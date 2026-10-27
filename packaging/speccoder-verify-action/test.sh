#!/usr/bin/env bash
# The action's step (verify.sh) against a fixture repository, a release on
# disk (file://) and a stand-in GitHub API on 127.0.0.1: the check is
# created on the pull request's head; a 403 falls back to the job's own
# output; a tampered archive, a wrong digest or a failed provenance check is
# refused before anything runs; neutral fails unless allowed; trust never
# comes from the checked commit (a push: the commit before it, or the
# default branch's tip; none: refused).
#
#   packaging/speccoder-verify-action/test.sh            # builds speccoder
#   SPECCODER_VERIFY_BIN=path/to/speccoder test.sh
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
work="$(mktemp -d)"
stub_pid=""
cleanup() {
  if [ -n "$stub_pid" ]; then { kill "$stub_pid" && wait "$stub_pid"; } 2>/dev/null || true; fi
  rm -rf "$work"
}
trap cleanup EXIT
fail() { echo "action-test: FAIL: $*" >&2; exit 1; }
for tool in jq curl python3 git; do
  command -v "$tool" >/dev/null || { echo "action-test: skipped (no $tool)"; exit 0; }
done
sum() { if command -v sha256sum >/dev/null; then sha256sum "$1"; else shasum -a 256 "$1"; fi | awk '{print $1}'; }

bin="${SPECCODER_VERIFY_BIN:-}"
if [ -z "$bin" ]; then
  cargo build --quiet --locked -p speccoder --manifest-path "$root/Cargo.toml"
  bin="${CARGO_TARGET_DIR:-$root/target}/debug/speccoder"
fi

# A release on disk, as release.yml lays it out.
version=9.8.7
case "$(uname -s) $(uname -m)" in
  "Linux x86_64") target=x86_64-unknown-linux-gnu ;;
  "Linux aarch64" | "Linux arm64") target=aarch64-unknown-linux-gnu ;;
  "Darwin arm64") target=aarch64-apple-darwin ;;
  *) echo "action-test: skipped (no build for $(uname -s) $(uname -m))"; exit 0 ;;
esac
pkg="speccoder-$version-$target"
release="$work/release/v$version"
mkdir -p "$release" "$work/$pkg"
cp "$bin" "$work/$pkg/speccoder"
tar -czf "$release/$pkg.tar.gz" -C "$work" "$pkg"
echo "$(sum "$release/$pkg.tar.gz")  $pkg.tar.gz" > "$release/SHA256SUMS"
good="$work/good.tar.gz"
cp "$release/$pkg.tar.gz" "$good"

# `gh attestation verify`, stood in: fails when told to.
mkdir -p "$work/bin"
cat > "$work/bin/gh" <<EOF
#!/bin/sh
[ -e "$work/provenance-fails" ] && { echo "no attestation matched" >&2; exit 1; }
echo "gh \$*" >> "$work/gh.log"
exit 0
EOF
chmod +x "$work/bin/gh"

# The repository: main with a unit; a pull request that hand-ticks a task,
# and one that touches no unit.
origin="$work/origin"
g() { git -C "$origin" -c user.name=Ada -c user.email=ada@example.com -c commit.gpgsign=false "$@"; }
mkdir -p "$origin/.specify/memory" "$origin/specs/001-login" "$origin/.github/speccoder"
echo "# Constitution" > "$origin/.specify/memory/constitution.md"
printf '# Spec\n\n## Requirements\n\n- **FR-001**: Users MUST log in.\n' > "$origin/specs/001-login/spec.md"
printf '# Tasks\n\n- [ ] T001 Log in (FR-001)\n' > "$origin/specs/001-login/tasks.md"
echo 'ada@example.com namespaces="in-toto" ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl' > "$origin/.github/speccoder/allowed_signers"
git -C "$origin" init -q -b main
g add -A && g commit -qm init
main="$(g rev-parse HEAD)"
g checkout -qb ticked
printf '# Tasks\n\n- [X] T001 Log in (FR-001)\n' > "$origin/specs/001-login/tasks.md"
g commit -qam "tick by hand"
ticked="$(g rev-parse HEAD)"
g checkout -q main
g checkout -qb docs
echo "# Readme" > "$origin/README.md"
g add -A && g commit -qm docs
docs="$(g rev-parse HEAD)"

# GitHub's check-runs API, stood in: records each body; 403 when told to.
cat > "$work/stub.py" <<'EOF'
import http.server, json, os, sys
work = sys.argv[1]
class H(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def handle_one(self, method):
        body = self.rfile.read(int(self.headers.get("Content-Length", 0)))
        n = len([f for f in os.listdir(work) if f.startswith("req-")])
        with open(os.path.join(work, f"req-{n:03d}-{method}"), "wb") as f:
            f.write(body)
        if os.path.exists(os.path.join(work, "api-forbidden")):
            code, answer = 403, {"message": "Resource not accessible by integration"}
        elif method == "POST":
            code, answer = 201, {"id": 7}
        else:
            code, answer = 200, {"id": 7}
        data = json.dumps(answer).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)
    def do_POST(self): self.handle_one("POST")
    def do_PATCH(self): self.handle_one("PATCH")
s = http.server.HTTPServer(("127.0.0.1", 0), H)
open(os.path.join(work, "port"), "w").write(str(s.server_port))
s.serve_forever()
EOF
mkdir -p "$work/api"
python3 -I "$work/stub.py" "$work/api" &
stub_pid=$!
for _ in $(seq 50); do [ -s "$work/api/port" ] && break; sleep 0.1; done
[ -s "$work/api/port" ] || fail "the API stub did not start"
api="http://127.0.0.1:$(cat "$work/api/port")"

event() { # head base
  printf '{"pull_request":{"head":{"sha":"%s","repo":{"clone_url":"https://example.invalid/fork.git"}},"base":{"sha":"%s"}},"repository":{"default_branch":"main"}}' "$1" "$2" > "$work/event.json"
}
run() { # extra env assignments as arguments
  : > "$work/output"
  env -u RUNNER_OS -u RUNNER_ARCH \
    PATH="$work/bin:$PATH" GITHUB_EVENT_NAME=pull_request GITHUB_EVENT_PATH="$work/event.json" \
    GITHUB_REPOSITORY=o/r GITHUB_API_URL="$api" GITHUB_OUTPUT="$work/output" GITHUB_STEP_SUMMARY="$work/summary" \
    RUNNER_TEMP="$work/tmp" SV_VERSION="$version" SV_TOKEN=t0ken SV_ALLOWED_SIGNERS=.github/speccoder/allowed_signers \
    SPECCODER_VERIFY_RELEASES="file://$work/release" SPECCODER_VERIFY_DIGESTS="$work/digests.txt" \
    SPECCODER_VERIFY_REMOTE="$origin" "$@" bash "$here/verify.sh" > "$work/log" 2>&1
}
posts() { find "$work/api" -name 'req-*-POST' | sort; }
last_post() { posts | tail -n 1; }
: > "$work/digests.txt"
mkdir -p "$work/tmp"

# 1. A hand tick: the check is created on the head, failure, job fails.
event "$ticked" "$main"
set +e; run; code=$?; set -e
[ "$code" = 1 ] || fail "hand tick: exit $code, want 1 ($(cat "$work/log"))"
body="$(last_post)"
[ -n "$body" ] || fail "no check run was created ($(cat "$work/log"))"
[ "$(jq -r .head_sha "$body")" = "$ticked" ] || fail "head_sha is not the pull request's head"
[ "$(jq -r .name "$body")" = "speccoder / evidence" ] || fail "wrong check name"
[ "$(jq -r .conclusion "$body")" = failure ] || fail "conclusion: $(jq -r .conclusion "$body")"
[ "$(jq -r '.output.annotations[0].annotation_level' "$body")" = failure ] || fail "no failure annotation"
grep -q '^conclusion=failure$' "$work/output" || fail "no conclusion output"
grep -q '^unverifiable=1$' "$work/output" || fail "no unverifiable count output"

# 2. The token cannot write checks: the job's own annotations and summary.
touch "$work/api/api-forbidden"
: > "$work/summary"
set +e; run; code=$?; set -e
rm "$work/api/api-forbidden"
[ "$code" = 1 ] || fail "fallback: exit $code, want 1"
grep -q "^::error file=specs/001-login/tasks.md,line=3" "$work/log" || fail "fallback: no workflow annotation ($(cat "$work/log"))"
grep -q "the named check was not published: HTTP 403" "$work/log" || fail "fallback: not said why"
grep -q "## speccoder / evidence: failure" "$work/summary" || fail "fallback: no job summary"

# 3. Nothing to check: published as failure, titled neutral; allowed, neutral.
event "$docs" "$main"
set +e; run; code=$?; set -e
[ "$code" = 1 ] || fail "neutral: exit $code, want 1"
body="$(last_post)"
[ "$(jq -r .conclusion "$body")" = failure ] || fail "neutral published as $(jq -r .conclusion "$body")"
[ "$(jq -r .output.title "$body")" = "neutral: no ticks checked" ] || fail "neutral title"
set +e; run SV_ALLOW_NEUTRAL=true; code=$?; set -e
[ "$code" = 0 ] || fail "allow-neutral: exit $code, want 0"
[ "$(jq -r .conclusion "$(last_post)")" = neutral ] || fail "allow-neutral: not neutral"

# 4. Refused before it runs: a failed provenance check, a wrong digest, a
# tampered archive. No check run is created.
before="$(posts | wc -l)"
touch "$work/provenance-fails"
set +e; run; code=$?; set -e
rm "$work/provenance-fails"
[ "$code" = 1 ] || fail "provenance: exit $code"
grep -q "build provenance" "$work/log" || fail "provenance: no reason"
echo "$version $target 0000000000000000000000000000000000000000000000000000000000000000" > "$work/digests.txt"
set +e; run; code=$?; set -e
: > "$work/digests.txt"
[ "$code" = 1 ] || fail "digest table: exit $code"
grep -q "checksum mismatch" "$work/log" || fail "digest table: no mismatch"
echo garbage >> "$release/$pkg.tar.gz"
set +e; run; code=$?; set -e
cp "$good" "$release/$pkg.tar.gz"
[ "$code" = 1 ] || fail "tampered: exit $code"
grep -q "checksum mismatch" "$work/log" || fail "tampered: no mismatch"
[ "$(posts | wc -l)" = "$before" ] || fail "a refused download still ran"

# 5. pull_request_target with the head checked out: refused.
event "$ticked" "$main"
git -C "$origin" checkout -q ticked
set +e; run GITHUB_EVENT_NAME=pull_request_target GITHUB_WORKSPACE="$origin"; code=$?; set -e
git -C "$origin" checkout -q main
[ "$code" = 1 ] || fail "pull_request_target with a checkout: exit $code"
grep -q "checked out in the workspace" "$work/log" || fail "pull_request_target: no reason"

# 6. Trust never comes from the checked commit itself. The branch adds a
# key of its own to allowed_signers; whatever the event, the check is
# given another commit's file.
g checkout -q docs
echo 'eve@example.com namespaces="in-toto" ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIEveEveEveEveEveEveEveEveEveEveEveEveEveEveEv' >> "$origin/.github/speccoder/allowed_signers"
g commit -qam "trust my own key"
own="$(g rev-parse HEAD)"
g checkout -q main
report="$work/tmp/speccoder-verify/report.json"
trusted_rev() { jq -r '.data.trust.ssh_allowed_signers.rev // empty' "$report"; }
push() { # head ref before
  printf '{"before":"%s","repository":{"default_branch":"main"}}' "$3" > "$work/event.json"
  set +e; run GITHUB_EVENT_NAME=push GITHUB_SHA="$1" GITHUB_REF="$2" SV_ALLOW_NEUTRAL=true; code=$?; set -e
}
# A push to another branch: the default branch's tip.
push "$own" refs/heads/docs "$docs"
[ "$code" = 0 ] || fail "push to a branch: exit $code ($(cat "$work/log"))"
[ "$(trusted_rev)" = "$main" ] || fail "push to a branch trusted $(trusted_rev), not main's tip $main"
grep -q "trust from the default branch's tip" "$work/log" || fail "push to a branch: trust not said"
# A push to the default branch: the commit before it.
push "$own" refs/heads/main "$main"
[ "$code" = 0 ] || fail "push to main: exit $code ($(cat "$work/log"))"
[ "$(trusted_rev)" = "$main" ] || fail "push to main trusted $(trusted_rev), not the commit before $main"
# The push that creates the default branch: no commit before it, refused.
for before in "" 0000000000000000000000000000000000000000; do
  push "$own" refs/heads/main "$before"
  [ "$code" = 1 ] || fail "push creating main (before '$before'): exit $code, want 1"
  grep -q "no earlier commit to take trust from" "$work/log" || fail "push creating main: no reason ($(cat "$work/log"))"
done
# A push whose trust commit is the head itself: refused, even with allow-neutral.
push "$main" refs/heads/docs "$docs"
[ "$code" = 1 ] || fail "push of main's tip to a branch: exit $code, want 1"
grep -q "is the checked commit itself" "$work/log" || fail "trust == head: no reason"
push "$own" refs/heads/main "$own"
[ "$code" = 1 ] || fail "push to main with before == head: exit $code, want 1"
# A pull request: its base, never its head, and never a base that is the head.
event "$own" "$main"
set +e; run SV_ALLOW_NEUTRAL=true; code=$?; set -e
[ "$code" = 0 ] || fail "pull request: exit $code ($(cat "$work/log"))"
[ "$(trusted_rev)" = "$main" ] || fail "pull request trusted $(trusted_rev), not its base"
event "$own" "$own"
set +e; run SV_ALLOW_NEUTRAL=true; code=$?; set -e
[ "$code" = 1 ] || fail "pull request with base == head: exit $code, want 1"
event "$own" ""
set +e; run SV_ALLOW_NEUTRAL=true; code=$?; set -e
[ "$code" = 1 ] || fail "pull request without a base: exit $code, want 1"

# 7. A release from before the check moved into speccoder (no
# `attest verify --branch --github-check`): refused, nothing published.
old=0.2.0
opkg="speccoder-$old-$target"
mkdir -p "$work/release/v$old" "$work/$opkg"
printf '#!/bin/sh\necho "Usage: speccoder attest verify [REV]"\n' > "$work/$opkg/speccoder"
chmod +x "$work/$opkg/speccoder"
tar -czf "$work/release/v$old/$opkg.tar.gz" -C "$work" "$opkg"
echo "$(sum "$work/release/v$old/$opkg.tar.gz")  $opkg.tar.gz" > "$work/release/v$old/SHA256SUMS"
event "$ticked" "$main"
before="$(posts | wc -l)"
set +e; run SV_VERSION="$old"; code=$?; set -e
[ "$code" = 1 ] || fail "an old release: exit $code, want 1"
grep -q "has no \`attest verify --branch --github-check\`" "$work/log" || fail "an old release: no reason ($(cat "$work/log"))"
[ "$(posts | wc -l)" = "$before" ] || fail "an old release still published a check"

echo "action-test: ok"
