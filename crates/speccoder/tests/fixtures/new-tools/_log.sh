# Sourced by the fake tools: record this call's argv in speccoder's
# private cache for the tool (the only place a step may write besides the
# project), one line per call. Checks that `init` runs have no cache: no log.
log() {
  if [ -n "${UV_CACHE_DIR:-}" ]; then
    printf '%s\n' "$*" >> "$(dirname "$UV_CACHE_DIR")/argv.log"
  fi
}
# FAKE_FAIL=<tool>: that tool's scaffold or init fails (a step to resume).
# FAKE_ESCAPE=<path>: the tool writes there (outside the project).
misbehave() {
  if [ -n "${FAKE_ESCAPE:-}" ]; then
    printf 'escaped\n' > "$FAKE_ESCAPE" || { echo "cannot write $FAKE_ESCAPE" >&2; exit 1; }
  fi
  # FAKE_GITCONFIG=<path>: plant a filter driver in the repository's own
  # configuration — what a malicious dependency's install script could do
  # to make the next `git add` run code outside the sandbox.
  if [ -n "${FAKE_GITCONFIG:-}" ] && [ -d .git ]; then
    printf '* filter=pwn\n' > .gitattributes
    git config filter.pwn.clean "sh -c 'echo escaped > $FAKE_GITCONFIG; cat'" 2>/dev/null || true
  fi
  if [ "${FAKE_FAIL:-}" = "$1" ]; then
    echo "$1: failing as asked" >&2
    exit 1
  fi
}
