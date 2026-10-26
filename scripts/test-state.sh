#!/usr/bin/env bash
# A nextest setup script: every test process gets a fresh, empty state
# directory, so no test — and no binary a test spawns — writes to the
# developer's real one (`~/.local/state/speccoder`). Whatever the shell had
# set is replaced on purpose: tests never use a real state directory.
set -euo pipefail
# Under `just test` the directory lives in the recipe's root, removed after.
root="${SPECCODER_TEST_STATE_ROOT:-${TMPDIR:-/tmp}}"
dir="$(mktemp -d "$root/speccoder-test-state.XXXXXX")"
{
  echo "SPECCODER_STATE_DIR=$dir/speccoder"
  echo "XDG_STATE_HOME=$dir/xdg"
} >> "$NEXTEST_ENV"
