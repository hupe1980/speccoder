#!/bin/sh
# Exits 2 on an unmitigated critical threat (as threatspec's check does),
# 0 once MITIGATED exists in the feature's folder. SARIF to --output, else stdout.
dir=""; out=""
while [ $# -gt 0 ]; do
  case "$1" in
    --feature-dir) dir="$2"; shift 2 ;;
    --output) out="$2"; shift 2 ;;
    *) shift ;;
  esac
done
if [ -f "$dir/MITIGATED" ]; then results="[]"; code=0; else
  results='[{"ruleId": "TS-C1", "level": "error", "message": {"text": "T-001 tampering with the session token is unmitigated"}, "locations": [{"physicalLocation": {"artifactLocation": {"uri": "specs/001-graph/spec.md"}, "region": {"startLine": 3}}}]}]'
  code=2
fi
log="{\"version\": \"2.1.0\", \"runs\": [{\"tool\": {\"driver\": {\"name\": \"threatspec-like\"}}, \"results\": $results}]}"
if [ -n "$out" ]; then printf '%s\n' "$log" > "$out"; else printf '%s\n' "$log"; fi
[ "$code" -ne 0 ] && echo "1 critical threat unmitigated" >&2
exit $code
