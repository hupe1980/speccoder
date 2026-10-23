#!/usr/bin/env bash
# The release scripts on a scratch repository: CHANGELOG sections, the tag
# preflight, the version bump, the formula and the publish guards.
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
scripts="$root/scripts/release"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
fail() { echo "release-test: FAIL: $*" >&2; exit 1; }
refuses() { # <expected text> <command…>
  local want="$1"; shift
  if out="$("$@" 2>&1)"; then fail "accepted: $* ($out)"; fi
  grep -q -- "$want" <<<"$out" || fail "$*: wanted \"$want\", got: $out"
}

# A scratch workspace: one crate, a CHANGELOG, a main branch on an "origin".
repo="$work/repo"
mkdir -p "$repo/crates/speccoder/src"
cd "$repo"
cat > Cargo.toml <<'TOML'
[workspace]
resolver = "3"
members = ["crates/*"]

[workspace.package]
version = "0.1.0-dev"
edition = "2024"

[workspace.dependencies]
speccoder-core = { path = "crates/speccoder-core", version = "0.1.0-dev" }
TOML
mkdir -p crates/speccoder-core/src
printf '[package]\nname = "speccoder-core"\nversion.workspace = true\nedition.workspace = true\n' > crates/speccoder-core/Cargo.toml
printf '' > crates/speccoder-core/src/lib.rs
printf '[package]\nname = "speccoder"\nversion.workspace = true\nedition.workspace = true\n\n[dependencies]\nspeccoder-core.workspace = true\n' > crates/speccoder/Cargo.toml
printf 'fn main() {}\n' > crates/speccoder/src/main.rs
cat > CHANGELOG.md <<'MD'
# Changelog

## [Unreleased]

### Added

- Everything.

## [0.0.1] - 2026-10-20

### Added

- A name.

[Unreleased]: https://github.com/hupe1980/speccoder/compare/v0.0.1...HEAD
[0.0.1]: https://github.com/hupe1980/speccoder/releases/tag/v0.0.1
MD
printf 'target/\n' > .gitignore
cargo generate-lockfile --quiet
g() { git -c user.name=t -c user.email=t@example.com -c commit.gpgsign=false -c tag.gpgsign=false "$@"; }
g -c init.defaultBranch=main init -q
g add -A && g commit -qm init
g update-ref refs/remotes/origin/main HEAD

# CHANGELOG sections.
[ "$("$scripts/changelog-section.sh" 0.0.1)" = "$(printf '### Added\n\n- A name.')" ] || fail "section 0.0.1"
"$scripts/changelog-section.sh" 0.1.0-rc.1 | grep -q "Everything" || fail "pre-release falls back to [Unreleased]"
refuses "no CHANGELOG section for 0.2.0" "$scripts/changelog-section.sh" 0.2.0

# Bump: pre-release leaves the CHANGELOG alone.
before="$(cat CHANGELOG.md)"
"$scripts/bump.sh" 0.1.0-rc.1 >/dev/null
[ "$(cat CHANGELOG.md)" = "$before" ] || fail "a pre-release bump changed the CHANGELOG"
grep -q '^version = "0.1.0-rc.1"' Cargo.toml || fail "workspace version not set"
grep -q 'speccoder-core = { path = "crates/speccoder-core", version = "0.1.0-rc.1" }' Cargo.toml || fail "internal dependency version not set"
changed="$(git status --porcelain | awk '{print $2}' | sort | tr '\n' ' ')"
[ "$changed" = "Cargo.lock Cargo.toml " ] || fail "bump touched: $changed"
refuses "not clean" "$scripts/bump.sh" 0.1.0
g commit -qam "rc"
g update-ref refs/remotes/origin/main HEAD

# Preflight.
PREFLIGHT_MAIN=origin/main "$scripts/preflight.sh" v0.1.0-rc.1 > "$work/out" || fail "preflight of a good rc tag"
grep -qx "prerelease=true" "$work/out" || fail "rc is a pre-release"
grep -qx "version=0.1.0-rc.1" "$work/out" || fail "preflight version"
refuses "workspace version is 0.1.0-rc.1" "$scripts/preflight.sh" v0.1.0
refuses "not v<semver>" "$scripts/preflight.sh" 0.1.0
g checkout -q -b side && printf 'x\n' > side.txt && g add side.txt && g commit -qm side
refuses "not on origin/main" env PREFLIGHT_MAIN=origin/main "$scripts/preflight.sh" v0.1.0-rc.1
g checkout -q main

# Bump: a release cuts [Unreleased] and fixes the links.
"$scripts/bump.sh" 0.1.0 >/dev/null
grep -q "^## \[0.1.0\] - $(date -u +%Y-%m-%d)$" CHANGELOG.md || fail "no dated 0.1.0 section"
[ "$(grep -c '^## \[Unreleased\]$' CHANGELOG.md)" = 1 ] || fail "[Unreleased] not re-created once"
grep -qx '\[Unreleased\]: https://github.com/hupe1980/speccoder/compare/v0.1.0...HEAD' CHANGELOG.md || fail "Unreleased link"
grep -qx '\[0.1.0\]: https://github.com/hupe1980/speccoder/compare/v0.0.1...v0.1.0' CHANGELOG.md || fail "0.1.0 link"
"$scripts/changelog-section.sh" 0.1.0 | grep -q "Everything" || fail "0.1.0 notes"
g commit -qam "release 0.1.0"
refuses "already at 0.1.0" "$scripts/bump.sh" 0.1.0
refuses "nothing under \[Unreleased\]" "$scripts/bump.sh" 0.2.0
refuses "not semver" "$scripts/bump.sh" banana

# Publish guards (nothing reaches crates.io: each refuses first).
refuses "not tagged v0.1.0" "$scripts/publish.sh" speccoder-core
g tag v0.1.0
printf 'dirty\n' > dirty.txt
refuses "not clean" "$scripts/publish.sh" speccoder-core
rm dirty.txt
"$scripts/bump.sh" 0.2.0-rc.1 >/dev/null && g commit -qam rc2 && g tag v0.2.0-rc.1
refuses "is a pre-release" "$scripts/publish.sh" speccoder-core

# Formula: every checksum substituted, no placeholder left.
cd "$work"
for t in aarch64-apple-darwin x86_64-apple-darwin aarch64-unknown-linux-gnu x86_64-unknown-linux-gnu; do
  printf '%064d  speccoder-0.1.0-%s.tar.gz\n' "${#t}" "$t"
done > SHA256SUMS
"$scripts/formula.sh" 0.1.0 SHA256SUMS > speccoder.rb || fail "formula"
[ "$(grep -c 'sha256 "0\{40\}' speccoder.rb)" = 4 ] || fail "four checksums"
grep -q 'version "0.1.0"' speccoder.rb || fail "formula version"
! grep -q '@' speccoder.rb || fail "placeholder left in the formula"
head -n 3 SHA256SUMS > short
refuses "no checksum for" "$scripts/formula.sh" 0.1.0 short

echo "release-test: ok"
