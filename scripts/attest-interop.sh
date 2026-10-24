#!/usr/bin/env bash
# Check speccoder's attestations with other people's tools, not speccoder's:
# every line of a bundle is a DSSE envelope whose SSH signature verifies
# with plain `ssh-keygen -Y verify -n in-toto` over the PAE rebuilt by hand,
# and whose payload is an in-toto Statement v1 that in-toto's own Go
# library (github.com/in-toto/attestation/go/v1) accepts.
#
#   scripts/attest-interop.sh                      # a fresh signed tick
#   scripts/attest-interop.sh bundle.jsonl allowed_signers
#
# Without arguments it has speccoder's test suite leave one signed tick's
# bundle behind (SPECCODER_INTEROP_OUT). Needs jq, ssh-keygen and, for the
# statement check, go (and the network to fetch the library). Not a
# `just ci` gate.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

if [ $# -eq 0 ]; then
  SPECCODER_INTEROP_OUT="$work/fixture" cargo test --quiet --manifest-path "$root/Cargo.toml" \
    -p speccoder-runtime --test attest a_tick_leaves_signed_statements_that_verify >/dev/null
  bundle="$work/fixture/bundle.jsonl"
  allowed="$work/fixture/allowed_signers"
else
  bundle="${1:?usage: attest-interop.sh [bundle.jsonl allowed_signers]}"
  allowed="${2:?usage: attest-interop.sh [bundle.jsonl allowed_signers]}"
fi

n=0
signed=0
while IFS= read -r line; do
  n=$((n + 1))
  type="$(jq -r .payloadType <<<"$line")"
  jq -r .payload <<<"$line" | base64 -d >"$work/body.$n"
  [ "$type" = "application/vnd.in-toto+json" ] || { echo "line $n: payloadType $type" >&2; exit 1; }
  # PAE(type, body) = "DSSEv1" SP len(type) SP type SP len(body) SP body
  { printf 'DSSEv1 %d %s %d ' "${#type}" "$type" "$(wc -c <"$work/body.$n" | tr -d ' ')"; cat "$work/body.$n"; } >"$work/pae.$n"
  sig="$(jq -r '.signatures[0].sig // empty' <<<"$line")"
  if [ -n "$sig" ]; then
    base64 -d <<<"$sig" >"$work/sig.$n"
    principal="$(ssh-keygen -Y find-principals -s "$work/sig.$n" -f "$allowed")"
    ssh-keygen -Y verify -f "$allowed" -I "$principal" -n in-toto -s "$work/sig.$n" <"$work/pae.$n" >/dev/null
    signed=$((signed + 1))
  fi
done <"$bundle"
echo "ssh-keygen: $signed of $n line(s) signed, every signature verifies over the rebuilt PAE"

if command -v go >/dev/null; then
  mkdir -p "$work/check"
  cat >"$work/check/main.go" <<'GO'
package main

import (
	"fmt"
	"os"

	v1 "github.com/in-toto/attestation/go/v1"
	"google.golang.org/protobuf/encoding/protojson"
)

func main() {
	for _, path := range os.Args[1:] {
		body, err := os.ReadFile(path)
		if err != nil {
			panic(err)
		}
		var s v1.Statement
		if err := protojson.Unmarshal(body, &s); err != nil {
			fmt.Fprintf(os.Stderr, "%s: %v\n", path, err)
			os.Exit(1)
		}
		if err := s.Validate(); err != nil {
			fmt.Fprintf(os.Stderr, "%s: %v\n", path, err)
			os.Exit(1)
		}
	}
	fmt.Printf("in-toto attestation (go/v1): %d statement(s) valid\n", len(os.Args)-1)
}
GO
  (
    cd "$work/check"
    go mod init interop >/dev/null 2>&1
    go get github.com/in-toto/attestation@v1.2.0 google.golang.org/protobuf@v1.36.12 >/dev/null 2>&1
    go run . "$work"/body.*
  )
else
  echo "go: not installed; the statements were not checked with in-toto's library" >&2
fi
