#!/usr/bin/env bash
# SC-003: `speccoder attest verify --branch` checks a feature of 50 signed
# ticks in under 10 seconds, the download of its binary included. The
# binary (a release build) is archived and served over HTTP on 127.0.0.1
# the way the evidence action downloads a release; the fixture is the
# branch check's own (crates/speccoder/tests/branch_fixture), built before
# the clock starts.
#
#   scripts/verify-performance.sh target/<target>/release/speccoder
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
bin="${1:?usage: verify-performance.sh <speccoder binary>}"
work="$(mktemp -d)"
pid=""
cleanup() {
  if [ -n "$pid" ]; then { kill "$pid" && wait "$pid"; } 2>/dev/null || true; fi
  rm -rf "$work"
}
trap cleanup EXIT
mkdir -p "$work/serve/pkg" "$work/got"
cp "$bin" "$work/serve/pkg/speccoder"
tar -czf "$work/serve/speccoder.tar.gz" -C "$work/serve" pkg
echo "verify-performance: binary $(wc -c < "$bin") bytes"
(cd "$work/serve" && exec python3 -I -u -m http.server 0 --bind 127.0.0.1) > "$work/http.log" 2>&1 &
pid=$!
for _ in $(seq 50); do grep -qo 'port [0-9]*' "$work/http.log" && break; sleep 0.1; done
port="$(grep -o 'port [0-9]*' "$work/http.log" | awk '{print $2}' || true)"
[ -n "$port" ] || { echo "verify-performance: the server did not start" >&2; exit 1; }
# The fixture first: building it is not the check's time.
cargo test --quiet --locked --manifest-path "$root/Cargo.toml" -p speccoder --test branch_check --no-run
start=$(date +%s%N 2>/dev/null || python3 -c 'import time; print(time.time_ns())')
curl -fsS -o "$work/got/v.tar.gz" "http://127.0.0.1:$port/speccoder.tar.gz"
tar -xzf "$work/got/v.tar.gz" -C "$work/got"
end=$(date +%s%N 2>/dev/null || python3 -c 'import time; print(time.time_ns())')
download_ms=$(( (end - start) / 1000000 ))
echo "verify-performance: download ${download_ms} ms"
SPECCODER_VERIFY_PERF_BIN="$work/got/pkg/speccoder" SPECCODER_VERIFY_PERF_BUDGET_MS=$((10000 - download_ms)) \
  cargo test --quiet --locked --manifest-path "$root/Cargo.toml" -p speccoder --test branch_check \
  -- --ignored --exact fifty_signed_ticks_are_checked_within_the_budget --nocapture
echo "verify-performance: ok"
