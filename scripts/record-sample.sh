#!/usr/bin/env bash
# Record the sample that `speccoder sample` and Try the Sample replay: a
# new Rust project made by `speccoder new`, its first feature planned from
# scripts/sample/, approved and run on a real model — then its history
# (git bundle), journal and manifest are written to
# crates/speccoder-runtime/assets/sample/, where the build bundles them.
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
idea="A Rust library with one function, word_count(text: &str) -> usize, that counts the words of a text: a word is a run of letters or digits; punctuation and whitespace separate words and are never counted; an empty text or one with only punctuation has 0 words."
project="$work/wordcount"
# A result's own words: its message, a failed skill's message, its code and
# fix lines — or its raw output when it is not a result.
said() {
  jq -r '[.message, (.data.stop | objects | .message // empty), (.code // empty | "code: \(.)"), ((.fix // [])[] | "fix: \(.)")] | map(select(. != null and . != "")) | reduce .[] as $x ([]; if any(.[]; . == $x) then . else . + [$x] end) | join("\n  ")' "$1" 2>/dev/null \
    || { printf 'not a result: '; head -c 2000 "$1"; }
}

"$bin" new "$project" --idea "$idea" --stack rust --yes
cd "$project"
# The planning is fixed (scripts/sample/): the model's planning skills vary
# run to run, and a question or an unapproved command honestly stops an
# unattended recording. What is recorded live is the run.
unit="001-word-count"
mkdir -p "specs/$unit"
cp "$root/scripts/sample/$unit/"*.md "specs/$unit/"
printf '{"feature_directory":"specs/%s"}\n' "$unit" > .specify/feature.json
git add -- "specs/$unit"
git commit -q -m "plan $unit"
"$bin" --json review approve --run autopilot --feature "$unit" > "$work/run.json" || true
runs=()
while IFS= read -r id; do runs+=("$id"); done < <(jq -r '.. | objects | select(has("run_id")) | .run_id | select(. != "")' "$work/run.json" | sort -u)
[ "${#runs[@]}" -gt 0 ] || { echo "record-sample: the run recorded nothing: $(said "$work/run.json")" >&2; exit 1; }
# A recording shows work: at least one task ticked `verified` — else it is empty.
"$root/scripts/sample-verified.sh" "$work/run.json" "$project"
journal="$(find "$SPECCODER_STATE_DIR" -name journal.redb | head -1)"
[ -n "$journal" ] || { echo "record-sample: no journal" >&2; exit 1; }

agentplane="$(cd "$root" && cargo metadata --format-version 1 --locked | jq -r '.packages[] | select(.name == "agentplane") | .version')"
speccoder="$(cd "$root" && cargo metadata --format-version 1 --no-deps --locked | jq -r '.packages[] | select(.name == "speccoder") | .version')"
read -r provider model < <(sed -n '/^\[model\]/,/^\[/{s/^provider = "\(.*\)"/\1/p;s/^model = "\(.*\)"/\1/p;}' speccoder.toml | paste -sd' ' -)
rm -rf "$dest"
mkdir -p "$dest"
git bundle create "$dest/sample.bundle" --all
gzip -9 -c "$journal" > "$dest/journal.redb.gz"
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
# Credentials, looked for where they are readable (the bundle is a
# compressed pack): every revision, diff and note of the repository, the
# journal and the manifest.
if ! "$root/scripts/sample-credentials.sh" "$project" "$journal" "$dest/manifest.toml"; then
  rm -rf "$dest"; exit 1
fi
echo "record-sample: ${#runs[@]} run(s) → $dest (${size} KiB); rebuild, then: cargo nextest run -p speccoder-runtime --test sample"
