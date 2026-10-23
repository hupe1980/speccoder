#!/usr/bin/env bash
# Generate a compatibility fixture with one OpenSpec release.
#
#   scripts/openspec-fixture.sh 1.14.3
#
# Runs that release's `openspec init` (npx @fission-ai/openspec@<version>,
# telemetry off) in a scratch directory, adds the 1.14.3 sample change and
# main spec, and copies the result to
# crates/speccoder-openspec/tests/fixtures/<version>/sample/ together with
# that release's JSON for `list`, `validate` and `instructions apply`
# (outputs/). The cross-check compares speccoder's parse with that JSON.
set -euo pipefail
version="${1:?usage: openspec-fixture.sh <version>}"
root="$(cd "$(dirname "$0")/.." && pwd)"
base="$root/crates/speccoder-openspec/tests/fixtures"
dest="$base/$version"
reference="$base/1.14.3/sample"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
export OPENSPEC_TELEMETRY=0 DO_NOT_TRACK=1 OPENSPEC_NO_UPDATE_CHECK=1 CI=1
openspec() { npx --yes "@fission-ai/openspec@$version" "$@"; }

cd "$work"
git -c init.defaultBranch=main init -q
openspec init --tools claude,agents --no-animation . >/dev/null
test -d openspec || { echo "openspec init $version produced no openspec/" >&2; exit 1; }

change=add-farewell-and-locale
mkdir -p openspec/specs/greeting
cp "$reference/openspec/specs/greeting/spec.md" openspec/specs/greeting/
cp -R "$reference/openspec/changes/$change" "openspec/changes/$change"

rm -rf "$dest"
mkdir -p "$dest/outputs"
openspec list --json > "$dest/outputs/list.json"
openspec validate "$change" --strict --json > "$dest/outputs/validate-change2.json" || true
openspec instructions apply --change "$change" --json > "$dest/outputs/instructions-apply-change2.json"
sed -i.bak "s#$work#/tmp/openspec-try/sample#g" "$dest"/outputs/*.json && rm -f "$dest"/outputs/*.bak

mkdir -p "$dest/sample"
cp -R openspec "$dest/sample/openspec"
for d in .claude .agents; do [ -d "$d" ] && cp -R "$d" "$dest/sample/$d"; done
echo "$version" > "$dest/VERSION"
echo "fixture: $dest"
