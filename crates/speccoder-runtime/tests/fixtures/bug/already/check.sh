#!/bin/sh
# A stand-in test runner: a test file names what `src/lib.rs` must say
# (`expect: <pattern>`); a missing pattern is an assertion failure, reported
# with libtest's summary line. `syntax` in a test file is a build error.
f="$1"
if grep -q '^syntax' "$f"; then
  echo 'error: expected one of `,`, `:`, or `}`, found `=`' >&2
  echo 'error: could not compile `notes` (test "repro") due to 1 previous error' >&2
  exit 101
fi
p=$(sed -n 's/^expect: //p' "$f")
if grep -q "$p" src/lib.rs; then
  echo 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out'
else
  echo "assertion failed: src/lib.rs does not $p"
  echo 'test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out'
  exit 101
fi
