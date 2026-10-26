#!/usr/bin/env bash
# A nextest wrapper script, run around every test process: each test gets a
# fresh, empty state directory of its own, removed when it ends, so no test
# — and no binary a test spawns — writes to the developer's real one
# (`~/.local/state/speccoder`) or shares state with another test. Whatever
# the shell had set is replaced on purpose. Git's background maintenance is
# off for every git the test runs, so no detached `gc` races the test.
set -uo pipefail
# Under `just test` the directories live in the recipe's root, removed after.
root="${SPECCODER_TEST_STATE_ROOT:-${TMPDIR:-/tmp}}"
dir="$(mktemp -d "${root%/}/speccoder-test-state.XXXXXX")" || exit 1
trap 'rm -rf "$dir"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
export SPECCODER_STATE_DIR="$dir/speccoder" XDG_STATE_HOME="$dir/xdg"
export GIT_CONFIG_COUNT=2 \
  GIT_CONFIG_KEY_0=maintenance.auto GIT_CONFIG_VALUE_0=false \
  GIT_CONFIG_KEY_1=gc.auto GIT_CONFIG_VALUE_1=0
"$@"
