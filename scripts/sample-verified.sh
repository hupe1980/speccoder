#!/usr/bin/env bash
# Whether a recorded run shows work: at least one task ticked `verified`,
# in the run's payload (`--json` result) and in a tick's trailers
# (`Verdict: verified`) of the project's repository. Exit 1 when not, with
# the run's own message, code and fix.
#
#   scripts/sample-verified.sh <run payload> <project repository>
set -euo pipefail
run="$1"; repo="$2"
verified="$(jq '[.data.attempts // [] | .[] | select(.ticked == true and .verdict.verdict == "verified")] | length' "$run" 2>/dev/null || echo 0)"
trailers="$(git -C "$repo" log --all --format='%(trailers:key=Verdict,valueonly)' | grep -cx 'verified' || true)"
if [ "$verified" -ge 1 ] && [ "${trailers:-0}" -ge 1 ]; then
  exit 0
fi
said="$(jq -r '[.message, (.code // empty | "code: \(.)"), ((.fix // [])[] | "fix: \(.)")] | map(select(. != null and . != "")) | join("\n  ")' "$run" 2>/dev/null)" \
  || said="not a result: $(head -c 2000 "$run")"
echo "sample-verified: the run ticked no task verified (payload: $verified, trailers: ${trailers:-0}): $said" >&2
exit 1
