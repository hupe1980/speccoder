#!/usr/bin/env bash
# Whether a recorded run shows work: every task ticked, at least one `verified`,
# in the run's payload (`--json` result) and in a tick's trailers
# (`Verdict: verified`) of the project's repository. Exit 1 when not, with
# the run's own message, code and fix.
#
#   scripts/sample-verified.sh <run payload> <project repository>
set -euo pipefail
run="$1"; repo="$2"
verified="$(jq '[.data.attempts // [] | .[] | select(.ticked == true and .verdict.verdict == "verified")] | length' "$run" 2>/dev/null || echo 0)"
trailers="$(git -C "$repo" log --all --format='%(trailers:key=Verdict,valueonly)' | grep -cx 'verified' || true)"
# Every task ticked: an unticked one is a stop, not a showcase.
open="$(git -C "$repo" for-each-ref --format='%(refname)' 'refs/heads/speccoder/*' | while read -r b; do git -C "$repo" show "$b:$(git -C "$repo" ls-tree -r --name-only "$b" | grep '/tasks.md$' | head -1)" 2>/dev/null; done | grep -c '^- \[ \]' || true)"
if [ "$verified" -ge 1 ] && [ "${trailers:-0}" -ge 1 ] && [ "${open:-0}" -eq 0 ]; then
  exit 0
fi
said="$(jq -r '[.message, (.code // empty | "code: \(.)"), ((.fix // [])[] | "fix: \(.)")] | map(select(. != null and . != "")) | join("\n  ")' "$run" 2>/dev/null)" \
  || said="not a result: $(head -c 2000 "$run")"
echo "sample-verified: the run ticked no task verified, or left tasks open (payload: $verified, trailers: ${trailers:-0}, open: ${open:-0}): $said" >&2
exit 1
