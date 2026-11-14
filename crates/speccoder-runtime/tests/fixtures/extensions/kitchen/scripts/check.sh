#!/bin/sh
# Leaves one finding; blocks when the feature has no ALLOW file.
out="${SPECCODER_FINDINGS:-/dev/stdout}"
cat > "$out" <<'SARIF'
{"version": "2.1.0", "runs": [{"tool": {"driver": {"name": "kitchen"}}, "results": [
  {"ruleId": "K-1", "level": "error", "message": {"text": "the kitchen is messy"},
   "locations": [{"physicalLocation": {"artifactLocation": {"uri": "src/lib.rs"}, "region": {"startLine": 1}}}]}]}]}
SARIF
if [ -n "$SPECCODER_FEATURE_DIR" ] && [ -f "$SPECCODER_FEATURE_DIR/ALLOW" ]; then exit 0; fi
echo "the kitchen is messy" >&2
exit 1
