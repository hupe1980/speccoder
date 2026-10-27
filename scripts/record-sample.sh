#!/usr/bin/env bash
# Record the sample that `speccoder sample` and Try the Sample replay
# (F-131): a new Rust project made by `speccoder new`, its first feature
# written and planned by Spec Kit's own skills, approved and run on a real
# model — then its history (git bundle), journal and manifest are written
# to crates/speccoder-runtime/assets/sample/, where the build bundles them.
# Needs a model key, `specify` and `cargo`; costs a few cents; never in CI.
#
#   set -a; . ./.env; set +a; just record-sample
set -euo pipefail
if [ -n "${CI:-}" ]; then echo "record-sample refuses to run in CI" >&2; exit 1; fi
if [ -z "${OPENAI_API_KEY:-}${ANTHROPIC_API_KEY:-}" ]; then
  echo "record-sample needs OPENAI_API_KEY or ANTHROPIC_API_KEY" >&2; exit 1
fi
for tool in jq specify cargo git; do
  command -v "$tool" >/dev/null || { echo "record-sample needs $tool" >&2; exit 1; }
done
root="$(cd "$(dirname "$0")/.." && pwd)"
dest="$root/crates/speccoder-runtime/assets/sample"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
export SPECCODER_STATE_DIR="$work/state"
cd "$root"
cargo build --quiet -p speccoder
bin="$root/target/debug/speccoder"
idea="A Rust library that counts the words of a text, ignoring punctuation"
project="$work/wordcount"

"$bin" new "$project" --idea "$idea" --stack rust --yes
cd "$project"
"$bin" --json spec specify "$idea" --through spec --no-prompt > "$work/spec.json" || true
unit="$(jq -r '.data.unit // empty' "$work/spec.json")"
[ -n "$unit" ] || { echo "record-sample: specify made no unit: $(jq -r .message "$work/spec.json")" >&2; exit 1; }
"$bin" review approve --feature "$unit"
"$bin" --json spec plan --through tasks --no-prompt --feature "$unit" > "$work/plan.json" || true
[ "$(jq -r '.data.stop.kind' "$work/plan.json")" = done ] || {
  echo "record-sample: planning stopped: $(jq -r .message "$work/plan.json")" >&2; exit 1; }
git add -- "specs/$unit"
git commit -q -m "plan $unit"
"$bin" --json review approve --run autopilot --feature "$unit" > "$work/run.json" || true
runs=()
while IFS= read -r id; do runs+=("$id"); done < <(jq -r '.. | objects | select(has("run_id")) | .run_id | select(. != "")' "$work/run.json" | sort -u)
[ "${#runs[@]}" -gt 0 ] || { echo "record-sample: the run recorded nothing: $(jq -r .message "$work/run.json")" >&2; exit 1; }
journal="$(find "$SPECCODER_STATE_DIR" -name journal.redb | head -1)"
[ -n "$journal" ] || { echo "record-sample: no journal" >&2; exit 1; }

agentplane="$(cd "$root" && cargo metadata --format-version 1 --locked | jq -r '.packages[] | select(.name == "agentplane") | .version')"
speccoder="$(cd "$root" && cargo metadata --format-version 1 --no-deps --locked | jq -r '.packages[] | select(.name == "speccoder") | .version')"
read -r provider model < <(sed -n '/^\[model\]/,/^\[/{s/^provider = "\(.*\)"/\1/p;s/^model = "\(.*\)"/\1/p;}' speccoder.toml | paste -sd' ' -)
rm -rf "$dest"
mkdir -p "$dest"
git bundle create "$dest/sample.bundle" --all
cp "$journal" "$dest/journal.redb"
ids="$(printf '"%s", ' "${runs[@]}")"
cat > "$dest/manifest.toml" <<TOML
case = "wordcount"
run_ids = [${ids%, }]
provider = "$provider"
model = "$model"
agentplane = "$agentplane"
speccoder = "$speccoder"
recorded_at = "$(date -u +%Y-%m-%d)"
expect = "reproduced"
TOML
size="$(du -sk "$dest" | cut -f1)"
[ "$size" -le 2048 ] || { echo "record-sample: $dest is ${size} KiB (> 2 MiB)" >&2; exit 1; }
if grep -aEq 'sk-(ant-)?[A-Za-z0-9_-]{20,}|Bearer [A-Za-z0-9._~+/-]{20,}|(OPENAI|ANTHROPIC)_API_KEY=\S' "$dest"/*; then
  echo "record-sample: something shaped like a credential is in the sample" >&2; exit 1
fi
echo "record-sample: ${#runs[@]} run(s) → $dest (${size} KiB); rebuild, then: cargo nextest run -p speccoder-runtime --test sample"
