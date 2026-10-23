#!/usr/bin/env bash
# Set the workspace version (and the internal crates' dependency versions);
# for a release (no `-`), cut `[Unreleased]` in CHANGELOG.md.
#
#   scripts/release/bump.sh 0.1.0       # release: CHANGELOG cut, links fixed
#   scripts/release/bump.sh 0.2.0-dev   # development: CHANGELOG untouched
#
# Never commits, tags or pushes; prints what to do next.
set -euo pipefail
version="${1:?usage: bump.sh <version>}"
repo="https://github.com/hupe1980/speccoder"
semver='^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z.-]+)?$'
if ! [[ "$version" =~ $semver ]]; then
  echo "bump: $version is not semver (e.g. 0.1.0, 0.2.0-rc.1)" >&2
  exit 1
fi
if [ -n "$(git status --porcelain)" ]; then
  echo "bump: the working tree is not clean; commit or stash first" >&2
  exit 1
fi
previous="$(sed -n '/^\[workspace.package\]/,/^\[/{s/^version = "\(.*\)"/\1/p;}' Cargo.toml | head -1)"
if [ "$previous" = "$version" ]; then
  echo "bump: the workspace is already at $version" >&2
  exit 1
fi

if [[ "$version" != *-* ]]; then
  if ! CHANGELOG=CHANGELOG.md "$(dirname "$0")/changelog-section.sh" Unreleased >/dev/null 2>&1; then
    echo "bump: CHANGELOG.md has nothing under [Unreleased] to release" >&2
    exit 1
  fi
fi

# Cargo.toml: the workspace version and every internal path dependency.
tmp="$(mktemp)"
awk -v v="$version" '
  /^\[/ { table = $0 }
  table == "[workspace.package]" && /^version = "/ { print "version = \"" v "\""; next }
  table == "[workspace.dependencies]" && /^speccoder-[a-z]+ = \{ path = / {
    sub(/version = "[^"]*"/, "version = \"" v "\"")
  }
  { print }
' Cargo.toml > "$tmp" && mv "$tmp" Cargo.toml
cargo update --workspace --quiet

if [[ "$version" != *-* ]]; then
  today="$(date -u +%Y-%m-%d)"
  last_release="$(grep -m1 -E '^## \[[0-9]' CHANGELOG.md | sed -E 's/^## \[([^]]+)\].*/\1/' || true)"
  tmp="$(mktemp)"
  awk -v v="$version" -v d="$today" -v prev="$last_release" -v repo="$repo" '
    /^## \[Unreleased\]/ { print "## [Unreleased]"; print ""; print "## [" v "] - " d; next }
    /^\[Unreleased\]: / {
      print "[Unreleased]: " repo "/compare/v" v "...HEAD"
      if (prev != "") print "[" v "]: " repo "/compare/v" prev "...v" v
      else print "[" v "]: " repo "/releases/tag/v" v
      next
    }
    { print }
  ' CHANGELOG.md > "$tmp" && mv "$tmp" CHANGELOG.md
fi

echo "bumped $previous → $version. Next:"
echo "  git commit -am \"release $version\" && git push origin main"
echo "  git tag -s v$version -m \"speccoder $version\" && git push origin v$version"
