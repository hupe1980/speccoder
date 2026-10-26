#!/usr/bin/env bash
# The speccoder-verify action's one step (action.yml passes its inputs as
# SV_*): download the pinned verifier and refuse it unless its build
# provenance and sha256 check out; fetch the pull request's head and its
# attestation notes as objects (nothing checked out, nothing from the
# pull request run); run the verifier with the base branch's trust and no
# token in its environment; publish the `speccoder / evidence` check with
# curl, or fall back to the job summary and workflow annotations.
#
# For tests: SPECCODER_VERIFY_RELEASES (the download base), SPECCODER_VERIFY_DIGESTS
# (the digest table) and SPECCODER_VERIFY_REMOTE (the repository to fetch from).
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
die() {
  echo "::error title=speccoder-verify::$*"
  echo "speccoder-verify: $*" >&2
  exit 1
}
out() { [ -n "${GITHUB_OUTPUT:-}" ] && echo "$1=$2" >> "$GITHUB_OUTPUT"; return 0; }
sum() { if command -v sha256sum >/dev/null; then sha256sum "$1"; else shasum -a 256 "$1"; fi | awk '{print $1}'; }

version="${SV_VERSION:?the version input is empty}"
version="${version#v}"
token="${SV_TOKEN:-}"
name="${SV_CHECK_NAME:-speccoder / evidence}"
event_name="${GITHUB_EVENT_NAME:-push}"
event="${GITHUB_EVENT_PATH:-/dev/null}"
jqe() { jq -r "$1 // empty" "$event" 2>/dev/null || true; }

# 1. The binary for this runner, verified before it runs.
case "${RUNNER_OS:-$(uname -s)} ${RUNNER_ARCH:-$(uname -m)}" in
  "Linux X64" | "Linux x86_64") target=x86_64-unknown-linux-musl ;;
  "Linux ARM64" | "Linux aarch64" | "Linux arm64") target=aarch64-unknown-linux-musl ;;
  "macOS ARM64" | "Darwin arm64") target=aarch64-apple-darwin ;;
  *) die "no speccoder-verify build for this runner (${RUNNER_OS:-?} ${RUNNER_ARCH:-?}); builds exist for Linux x64, Linux ARM64 and macOS ARM64" ;;
esac
work="${RUNNER_TEMP:-$(mktemp -d)}/speccoder-verify"
rm -rf "$work" && mkdir -p "$work"
releases="${SPECCODER_VERIFY_RELEASES:-https://github.com/hupe1980/speccoder/releases/download}"
pkg="speccoder-verify-$version-$target"
curl -fsSL --retry 3 -o "$work/$pkg.tar.gz" "$releases/v$version/$pkg.tar.gz" \
  || die "cannot download $pkg.tar.gz from $releases/v$version"
if ! GH_TOKEN="$token" gh attestation verify "$work/$pkg.tar.gz" \
  --repo hupe1980/speccoder \
  --signer-workflow hupe1980/speccoder/.github/workflows/release.yml \
  --source-ref "refs/tags/v$version" > "$work/provenance.log" 2>&1; then
  die "the build provenance of $pkg.tar.gz does not verify (signed by hupe1980/speccoder's release workflow for v$version): $(tail -n 1 "$work/provenance.log"); not run"
fi
digests="${SPECCODER_VERIFY_DIGESTS:-$here/digests.txt}"
expected="$(awk -v v="$version" -v t="$target" '$1 == v && $2 == t { print $3 }' "$digests" 2>/dev/null || true)"
if [ -z "$expected" ]; then
  curl -fsSL --retry 3 -o "$work/SHA256SUMS" "$releases/v$version/SHA256SUMS" \
    || die "cannot download SHA256SUMS for v$version"
  expected="$(awk -v f="$pkg.tar.gz" '$2 == f || $2 == "*" f { print $1 }' "$work/SHA256SUMS")"
fi
[ -n "$expected" ] || die "no sha256 for $pkg.tar.gz; not run"
actual="$(sum "$work/$pkg.tar.gz")"
[ "$actual" = "$expected" ] || die "checksum mismatch for $pkg.tar.gz: expected $expected, got $actual; not run"
tar -xzf "$work/$pkg.tar.gz" -C "$work"
bin="$work/$pkg/speccoder-verify"
[ -x "$bin" ] || die "$pkg.tar.gz holds no speccoder-verify"

# 2. What to check: the pull request's head (never github.sha, the base
# under pull_request_target), from the fork when it is one.
case "$event_name" in
  pull_request | pull_request_target)
    head="$(jqe .pull_request.head.sha)"
    base="$(jqe .pull_request.base.sha)"
    head_url="$(jqe .pull_request.head.repo.clone_url)"
    mode="${SV_MODE:-touched}"
    ;;
  *)
    head="${GITHUB_SHA:?}"
    base=""
    head_url=""
    mode="${SV_MODE:-all}"
    ;;
esac
[ -n "$head" ] || die "no head commit in the $event_name event"
if [ "$event_name" = pull_request_target ] && [ -n "${GITHUB_WORKSPACE:-}" ] \
  && [ "$(git -C "$GITHUB_WORKSPACE" rev-parse HEAD 2>/dev/null || true)" = "$head" ]; then
  die "the pull request's head is checked out in the workspace under pull_request_target: remove the checkout step, the verifier reads git objects only"
fi
base_url="${SPECCODER_VERIFY_REMOTE:-${GITHUB_SERVER_URL:-https://github.com}/${GITHUB_REPOSITORY:?}.git}"
if [ -z "$head_url" ] || [ -n "${SPECCODER_VERIFY_REMOTE:-}" ]; then
  head_url="$base_url"
fi
repo="$work/repo"
git init -q "$repo"
fetch() {
  local url="$1"
  shift
  if [[ "$url" == https://* && -n "$token" && "$url" == "$base_url" ]]; then
    git -C "$repo" -c "http.extraheader=AUTHORIZATION: basic $(printf 'x-access-token:%s' "$token" | base64 | tr -d '\n')" \
      fetch --no-tags --quiet "$url" "$@"
  else
    git -C "$repo" fetch --no-tags --quiet "$url" "$@"
  fi
}
fetch "$head_url" "$head" || die "cannot fetch $head from $head_url"
[ -z "$base" ] || fetch "$base_url" "$base" || die "cannot fetch the base $base"
if ! fetch "$head_url" "+refs/notes/speccoder:refs/notes/speccoder" 2>/dev/null; then
  echo "::warning title=speccoder-verify::no attestation notes (refs/notes/speccoder) at $head_url: every tick is unverifiable until they are pushed (git push origin refs/notes/speccoder)"
fi

# 3. The verifier: the base's trust, no token, nothing else in its environment.
trust_rev="${base:-$head}"
args=(check --repo "$repo" --head "$head" --json --github-check "$work/check.json" --check-name "$name"
  "--require-signed=${SV_REQUIRE_SIGNED:-true}")
if [ "$mode" = touched ] && [ -n "$base" ]; then args+=(--base "$base"); else args+=(--all); fi
if [ -n "${SV_IDENTITY:-}" ]; then
  args+=(--certificate-identity "$SV_IDENTITY" --certificate-oidc-issuer "${SV_ISSUER:-}")
fi
allowed="${SV_ALLOWED_SIGNERS:-.github/speccoder/allowed_signers}"
if git -C "$repo" cat-file -e "$trust_rev:$allowed" 2>/dev/null || [ -z "${SV_IDENTITY:-}" ]; then
  args+=(--allowed-signers-rev "$trust_rev:$allowed")
fi
[ "${SV_REQUIRE_VERIFIED:-false}" = true ] && args+=(--require-verified)
[ "${SV_ALLOW_NEUTRAL:-false}" = true ] && args+=(--allow-neutral)
verifier() {
  env -i PATH="$PATH" HOME="${HOME:-/tmp}" LANG="${LANG:-C.UTF-8}" \
    ${GNUPGHOME:+GNUPGHOME="$GNUPGHOME"} ${GITHUB_STEP_SUMMARY:+GITHUB_STEP_SUMMARY="$GITHUB_STEP_SUMMARY"} \
    "$bin" "$@"
}
set +e
verifier "${args[@]}" > "$work/report.json"
code=$?
set -e
out report "$work/report.json"
conclusion="$(jq -r '.data.conclusion // "failure"' "$work/report.json" 2>/dev/null || echo failure)"
out conclusion "$conclusion"
for k in verified regression_only attested stale unverifiable; do
  out "${k//_/-}" "$(jq -r ".data.totals.$k // 0" "$work/report.json" 2>/dev/null || echo 0)"
done
out share "$(jq -r '.data.share // empty' "$work/report.json" 2>/dev/null || true)"
if [ ! -s "$work/check.json" ]; then
  die "$(jq -r '.message // "the verifier refused"' "$work/report.json" 2>/dev/null || echo "the verifier refused")"
fi

# 4. The check run, on the head commit: one POST, then a PATCH per further
# batch of 50 annotations. Where the token cannot, the job's own output.
api="${GITHUB_API_URL:-https://api.github.com}"
call() {
  curl -sS -o "$work/response.json" -w '%{http_code}' -X "$1" \
    -H "Authorization: Bearer $token" -H "Accept: application/vnd.github+json" \
    -H "X-GitHub-Api-Version: 2022-11-28" "$api/repos/$GITHUB_REPOSITORY/$2" --data-binary @- || echo 000
}
status="$(jq -c .create "$work/check.json" | call POST check-runs)"
if [ "$status" = 201 ]; then
  id="$(jq -r .id "$work/response.json")"
  n="$(jq '.updates | length' "$work/check.json")"
  for ((i = 0; i < n; i++)); do
    s="$(jq -c ".updates[$i]" "$work/check.json" | call PATCH "check-runs/$id")"
    [ "$s" = 200 ] || echo "::warning title=speccoder-verify::annotation batch $((i + 1)) not published (HTTP $s)"
  done
  echo "published the check \"$name\" on $head"
else
  why="HTTP $status: $(jq -r '.message // empty' "$work/response.json" 2>/dev/null || true)"
  verifier "${args[@]}" --github-annotations > "$work/fallback.txt" || true
  grep '^::' "$work/fallback.txt" || true
  echo "::notice title=speccoder-verify::the named check was not published: $why (a pull_request run from a fork has a read-only token); this job's status carries the outcome"
fi

# 5. The badge: a push to the default branch only, on the maintainer's opt-in.
default_branch="$(jqe .repository.default_branch)"
if [ -n "${SV_BADGE_BRANCH:-}" ] && [ "$event_name" = push ] && [ -n "$default_branch" ] \
  && [ "${GITHUB_REF:-}" = "refs/heads/$default_branch" ]; then
  badge_args=(badge --repo "$repo" --head "$head" --svg "$work/speccoder-evidence.svg" --endpoint "$work/speccoder-evidence.json")
  [[ " ${args[*]} " == *" --allowed-signers-rev "* ]] && badge_args+=(--allowed-signers-rev "$trust_rev:$allowed")
  [ -n "${SV_IDENTITY:-}" ] && badge_args+=(--certificate-identity "$SV_IDENTITY" --certificate-oidc-issuer "${SV_ISSUER:-}")
  if verifier "${badge_args[@]}" > /dev/null; then
    svg="$(git -C "$repo" hash-object -w "$work/speccoder-evidence.svg")"
    endpoint="$(git -C "$repo" hash-object -w "$work/speccoder-evidence.json")"
    tree="$(printf '100644 blob %s\tspeccoder-evidence.json\n100644 blob %s\tspeccoder-evidence.svg\n' "$endpoint" "$svg" | git -C "$repo" mktree)"
    parent=()
    if fetch "$base_url" "+refs/heads/$SV_BADGE_BRANCH:refs/badge" 2>/dev/null; then parent=(-p refs/badge); fi
    commit="$(git -C "$repo" -c user.name=speccoder-verify -c user.email=speccoder-verify@users.noreply.github.com \
      commit-tree "$tree" "${parent[@]}" -m "evidence badge for $head")"
    git -C "$repo" -c "http.extraheader=AUTHORIZATION: basic $(printf 'x-access-token:%s' "$token" | base64 | tr -d '\n')" \
      push --quiet "$base_url" "$commit:refs/heads/$SV_BADGE_BRANCH" \
      || echo "::warning title=speccoder-verify::the badge was not pushed to $SV_BADGE_BRANCH (contents: write?)"
  fi
fi

# 6. The outcome: neutral fails the job unless allowed.
case "$code" in
  0) exit 0 ;;
  78)
    if [ "${SV_ALLOW_NEUTRAL:-false}" = true ]; then exit 0; fi
    echo "neutral: no ticks checked — not proof (set allow-neutral: true to pass it)"
    exit 1
    ;;
  *) exit 1 ;;
esac
