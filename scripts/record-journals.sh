#!/usr/bin/env bash
# Re-record the real-model journals that crates/speccoder-runtime/tests/recorded.rs
# replays, so an agentplane upgrade that cannot read or reproduce a real run
# fails CI. Needs a model key in the environment; costs a few cents; never in CI.
#
#   set -a; . ./.env; set +a; just record-journals
set -euo pipefail
if [ -n "${CI:-}" ]; then echo "record-journals refuses to run in CI" >&2; exit 1; fi
if [ -z "${OPENAI_API_KEY:-}${ANTHROPIC_API_KEY:-}" ]; then
  echo "record-journals needs OPENAI_API_KEY or ANTHROPIC_API_KEY" >&2; exit 1
fi
root="$(cd "$(dirname "$0")/.." && pwd)"
dest="$root/crates/speccoder-runtime/tests/fixtures/recorded"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
cd "$root"
cargo build --quiet -p speccoder
bin="$root/target/debug/speccoder"
agentplane="$(cargo metadata --format-version 1 --locked | jq -r '.packages[] | select(.name == "agentplane") | .version')"
speccoder="$(cargo metadata --format-version 1 --no-deps --locked | jq -r '.packages[] | select(.name == "speccoder") | .version')"
read -r provider model < <(sed -n '/^\[model\]/,/^\[/{s/^provider = "\(.*\)"/\1/p;s/^model = "\(.*\)"/\1/p;}' evals/eval.toml | paste -sd' ' -)

for case in calc-add impossible-add; do
  keep="$work/$case"
  "$bin" --json eval evals --case "$case" --keep "$keep" > "$work/$case.json" || true
  run_id="$(jq -r '.data.cases[0].run_id // empty' "$work/$case.json")"
  [ -n "$run_id" ] || { echo "record-journals: $case recorded no run" >&2; exit 1; }
  journal="$(find "$keep/$case/state" -name journal.redb | head -1)"
  [ -n "$journal" ] || { echo "record-journals: $case left no journal" >&2; exit 1; }
  out="$dest/$case"
  rm -rf "$out"
  mkdir -p "$out/journal"
  cp "$journal" "$out/journal/journal.redb"
  cp "$keep/$case/repo/speccoder.toml" "$out/speccoder.toml"
  cat > "$out/manifest.toml" <<TOML
case = "$case"
run_ids = ["$run_id"]
provider = "$provider"
model = "$model"
agentplane = "$agentplane"
speccoder = "$speccoder"
recorded_at = "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
expect = "reproduced"
TOML
  echo "record-journals: $case → $out ($(du -h "$out/journal/journal.redb" | cut -f1))"
done
