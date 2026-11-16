#!/usr/bin/env bash
# Build the showcase the docs' screenshots are taken from, in <dir>: a home
# of its own (<dir>/home, so paths read ~/code/…), speccoder state of its
# own (<dir>/state) and three projects in its recent list —
#   url-slugs   two features; T001–T003 verified on the real pipeline, T004
#               waiting for approval, T005 blocked, an open question and
#               requirements to review in both features
#   newsletter  requirements to review
#   style-guide nothing waits
# The model is scripts/showcase/mock.py (no key, no network, no cost).
# Nothing of this machine's state, home or git identity reaches it.
# Maintainer-run, never in CI; scripts/screenshots.cjs calls it.
#
#   cargo build -p speccoder && scripts/showcase/setup.sh "$(mktemp -d)"
set -euo pipefail
if [ -n "${CI:-}" ]; then echo "the showcase is not built in CI" >&2; exit 1; fi
for tool in git python3 cargo cargo-nextest; do
  command -v "$tool" >/dev/null || { echo "the showcase needs $tool" >&2; exit 1; }
done
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
bin="${SPECCODER_BIN:-$root/target/debug/speccoder}"
[ -n "${1:-}" ] || { echo "usage: setup.sh <empty dir>" >&2; exit 2; }
mkdir -p "$1"
dir="$(cd "$1" && pwd -P)"

# The toolchain stays where it is; everything else lives in <dir>.
export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}" RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
export HOME="$dir/home" SPECCODER_STATE_DIR="$dir/state" OPENAI_API_KEY=sk-showcase
unset XDG_STATE_HOME XDG_CONFIG_HOME ANTHROPIC_API_KEY
export GIT_CONFIG_NOSYSTEM=1 GIT_AUTHOR_NAME="Ada Lovelace" GIT_AUTHOR_EMAIL=ada@example.com
export GIT_COMMITTER_NAME="Ada Lovelace" GIT_COMMITTER_EMAIL=ada@example.com
mkdir -p "$HOME/code" "$SPECCODER_STATE_DIR" "$dir/mock"
git config --global user.name "Ada Lovelace"
git config --global user.email ada@example.com
git config --global init.defaultBranch main

python3 -I "$here/mock.py" "$here/plan.json" "$dir/mock" 2>"$dir/mock/err.log" &
mock=$!

trap 'kill $mock 2>/dev/null || true' EXIT
for _ in $(seq 50); do [ -s "$dir/mock/port" ] && break; sleep 0.1; done
port="$(cat "$dir/mock/port")"

# A Spec Kit project: constitution and init options from the slugify eval.
speckit() {
  mkdir -p "$1/.specify/memory"
  cp "$root/evals/slugify/project/.specify/init-options.json" "$1/.specify/"
  cp "$root/evals/slugify/project/.specify/memory/constitution.md" "$1/.specify/memory/"
}

# ── url-slugs ────────────────────────────────────────────────────────────
p="$HOME/code/url-slugs"
mkdir -p "$p/src" "$p/specs/001-url-slugs" "$p/specs/002-rss-feed"
speckit "$p"
cd "$p"
printf 'target/\n' > .gitignore
mkdir -p .config
printf '[profile.speccoder]\nfail-fast = false\n\n[profile.speccoder.junit]\npath = "junit.xml"\n' > .config/nextest.toml
cat > Cargo.toml <<'EOF'
[package]
name = "slugs"
version = "0.1.0"
edition = "2024"

[dependencies]
EOF
cat > src/lib.rs <<'EOF'
//! URL slugs for article titles.

/// The URL slug of an article title.
pub fn slugify(title: &str) -> String {
    title.to_owned()
}
EOF
cat > specs/001-url-slugs/spec.md <<'EOF'
# Feature Specification: URL slugs

**Feature Branch**: `001-url-slugs` · **Status**: Draft

## User Scenarios & Testing

### User Story 1 - Make a slug from a title (Priority: P1)

An editor publishes an article; its URL is made from the title.

**Acceptance Scenarios**:

1. **Given** the title `Hello, World`, **When** it is slugified, **Then** the slug is `hello-world`.
2. **Given** the title `  Rust  2024 `, **When** it is slugified, **Then** the slug is `rust-2024`.
3. **Given** the title `---`, **When** it is slugified, **Then** the slug is empty.

### User Story 2 - Titles in other languages (Priority: P2)

Titles with accents keep their letters.

**Acceptance Scenarios**:

1. **Given** the title `Crème brûlée`, **When** it is slugified, **Then** the slug is `creme-brulee`.
2. **Given** a title of 120 characters, **When** it is slugified, **Then** the slug has at most 60 characters and does not end with `-`.

## Requirements

### Functional Requirements

- **FR-001**: ASCII letters MUST become lowercase; ASCII digits MUST be kept.
- **FR-002**: Every run of other characters MUST become a single `-`.
- **FR-003**: A slug MUST NOT start or end with `-`.
- **FR-004**: Latin letters with accents MUST become their base letter (`é` → `e`).
- **FR-005**: A slug MUST be at most [NEEDS CLARIFICATION: 60 or 80 characters?] characters, cut at a word boundary.

## Success Criteria

- **SC-001**: Every title in the archive gets a stable, readable slug.
EOF
cat > specs/001-url-slugs/plan.md <<'EOF'
# Implementation Plan: URL slugs

**Language**: Rust 2024 · **Dependencies**: none · **Testing**: `cargo test`

`pub fn slugify(title: &str) -> String` in `src/lib.rs`; transliteration by a small table of Latin accents, no crates.
EOF
cat > specs/001-url-slugs/tasks.md <<'EOF'
# Tasks: URL slugs

## Phase 1: User Story 1 - Make a slug from a title (Priority: P1) 🎯 MVP

- [ ] T001 [US1] Implement `pub fn slugify(title: &str) -> String` in src/lib.rs: lowercase ASCII letters, keep digits (FR-001)
- [ ] T002 [US1] Collapse every run of other characters into one `-` in src/lib.rs (FR-002)
- [ ] T003 [US1] Trim leading and trailing `-` in src/lib.rs (FR-003)

## Phase 2: User Story 2 - Titles in other languages (Priority: P2)

- [ ] T004 [US2] Transliterate Latin accents in src/lib.rs (FR-004)
- [ ] T005 [US2] Cut slugs at a word boundary to the maximum length in src/lib.rs (FR-005)
EOF
cat > specs/002-rss-feed/spec.md <<'EOF'
# Feature Specification: RSS feed

## User Scenarios & Testing

### User Story 1 - Subscribe to new articles (Priority: P1)

A reader subscribes to the blog and sees each new article in their feed reader.

**Acceptance Scenarios**:

1. **Given** three published articles, **When** the feed is requested, **Then** it lists them newest first, each with its slug URL.

## Requirements

### Functional Requirements

- **FR-001**: The feed MUST be valid RSS 2.0.
- **FR-002**: Each item MUST link to the article by its slug.
EOF
cat > speccoder.toml <<EOF
[checks.test]
argv = ["cargo", "nextest", "run", "--workspace", "--profile", "speccoder", "--no-tests=warn"]
junit = "target/nextest/speccoder/junit.xml"
timeout_secs = 300

[model]
provider = "openai"
model = "gpt-5.5"
base_url = "http://127.0.0.1:$port/v1"

[commands]
allow = [["cargo", "test", "--quiet"]]

# T004's cargo add waits for you rather than failing offline in the sandbox.
[approvals]
contained = "ask"

[acceptance]
held_out = false
strength = false
EOF
git init -q
git add -A
git commit -qm "spec: URL slugs and RSS feed"
"$bin" review approve FR-001 FR-002 FR-003 --feature 001 >/dev/null
git add -A
git commit -qm "review: approve FR-001..FR-003"
for t in T001 T002 T003; do
  "$bin" run --feature 001 --task "$t" --no-prompt --plain >"$dir/run-$t.log" 2>&1 \
    || { echo "url-slugs $t did not verify: see $dir/run-$t.log" >&2; exit 1; }
done
# T004 asks to run `cargo add`: the run pauses for the approval.
"$bin" run --feature 001 --task T004 --no-prompt --plain >"$dir/run-T004.log" 2>&1 || true

# ── newsletter: requirements to review ───────────────────────────────────
p="$HOME/code/newsletter"
mkdir -p "$p/specs/001-weekly-digest"
speckit "$p"
cat > "$p/specs/001-weekly-digest/spec.md" <<'EOF'
# Feature Specification: Weekly digest

## User Scenarios & Testing

### User Story 1 - The week in one email (Priority: P1)

A subscriber gets one email a week listing the articles published that week.

**Acceptance Scenarios**:

1. **Given** four articles published this week, **When** the digest is sent, **Then** it lists the four, newest first.

## Requirements

### Functional Requirements

- **FR-001**: The digest MUST be sent once a week, on Monday at 08:00 in the subscriber's time zone.
- **FR-002**: A week without articles MUST send no digest.
EOF
(cd "$p" && git init -q && git add -A && git commit -qm "spec: weekly digest")

# ── style-guide: nothing waits ───────────────────────────────────────────
p="$HOME/code/style-guide"
mkdir -p "$p/specs/001-headings"
speckit "$p"
cat > "$p/specs/001-headings/spec.md" <<'EOF'
# Feature Specification: Headings

## Requirements

### Functional Requirements

- **FR-001**: Headings MUST use sentence case.
EOF
(cd "$p" && git init -q && git add -A && git commit -qm "spec: headings" \
  && "$bin" review approve FR-001 --feature 001 >/dev/null && git add -A && git commit -qm "review: approve FR-001")

# Opening a project puts it in the recent list: the other two first, so
# url-slugs (served for the shots) is the newest.
for name in style-guide newsletter; do
  log="$dir/ui-$name.log"
  (cd "$HOME/code/$name" && exec "$bin" ui --no-open --port 0 >"$log" 2>&1) &
  pid=$!
  for _ in $(seq 100); do grep -q 'token=' "$log" 2>/dev/null && break; sleep 0.1; done
  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
  sleep 1
done
echo "showcase ready in $dir"
