#!/usr/bin/env bash
# Generate a compatibility fixture with one Spec Kit release.
#
#   scripts/speckit-fixture.sh 1.2.3
#
# Runs that release's `specify init` (fetched with uvx from github/spec-kit)
# in a scratch directory and copies what speccoder reads into
# crates/speccoder-speckit/tests/fixtures/compat/<version>/: the .specify/
# tree (scripts, templates, memory, init options, workflows, extensions), the
# installed skills, and a feature built from that release's own templates.
set -euo pipefail
version="${1:?usage: speckit-fixture.sh <version>}"
root="$(cd "$(dirname "$0")/.." && pwd)"
dest="$root/crates/speccoder-speckit/tests/fixtures/compat/$version"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

cd "$work"
git -c init.defaultBranch=main init -q
uvx --quiet --from "git+https://github.com/github/spec-kit@v$version" \
  specify init --here --integration claude --script sh --force >/dev/null 2>&1 \
|| uvx --quiet --from "git+https://github.com/github/spec-kit@v$version" \
  specify init --here --ai claude --script sh --force >/dev/null 2>&1

test -d .specify || { echo "specify init $version produced no .specify/" >&2; exit 1; }

# A feature from the release's own templates.
feature="specs/001-sample"
mkdir -p "$feature"
for t in spec plan tasks; do
  src=".specify/templates/$t-template.md"
  [ -f "$src" ] && cp "$src" "$feature/$t.md"
done

rm -rf "$dest"
mkdir -p "$dest"
cp -R .specify "$dest/.specify"
[ -d .claude/skills ] && mkdir -p "$dest/.claude" && cp -R .claude/skills "$dest/.claude/skills"
cp -R specs "$dest/specs"
find "$dest" -name '__pycache__' -prune -exec rm -rf {} +
echo "$version" > "$dest/VERSION"
echo "fixture: $dest"
