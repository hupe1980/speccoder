#!/usr/bin/env bash
# Whether a sample about to be bundled holds something shaped like a
# credential: every revision of every file and every diff of the project's
# repository (the bundle itself is a compressed pack, unreadable to grep),
# its notes, and the journal and manifest as stored. Exit 1 when found.
#
#   scripts/sample-credentials.sh <project repository> <journal> <manifest>
set -euo pipefail
repo="$1"; shift
credential='sk-(ant-)?[A-Za-z0-9_-]{20,}|Bearer [A-Za-z0-9._~+/-]{20,}|(OPENAI|ANTHROPIC)_API_KEY=[^[:space:]]'
history() {
  git -C "$repo" log -p --all --format='%H %s%n%b'
  git -C "$repo" log --notes=speccoder -p --all 2>/dev/null || true
  # shellcheck disable=SC2046
  git -C "$repo" grep -I -h -e '' $(git -C "$repo" rev-list --all)
}
# Not `grep -q`: it would stop reading, git would die of SIGPIPE and
# pipefail would read that as "not found".
if history | grep -aE "$credential" >/dev/null; then
  echo "sample-credentials: the project's history holds something shaped like a credential" >&2
  exit 1
fi
for f in "$@"; do
  if grep -aEq "$credential" "$f"; then
    echo "sample-credentials: $f holds something shaped like a credential" >&2
    exit 1
  fi
done
