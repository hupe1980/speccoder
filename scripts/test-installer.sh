#!/usr/bin/env bash
# The installer against a fake release on disk (file://): a good archive
# installs, a tampered one is refused with nothing installed, an unsupported
# platform is refused.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
installer="$root/packaging/install.sh"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
fail() { echo "test-installer: FAIL: $*" >&2; exit 1; }

version=9.8.7
case "$(uname -s) $(uname -m)" in
  "Darwin arm64") target=aarch64-apple-darwin ;;
  "Darwin x86_64") target=x86_64-apple-darwin ;;
  "Linux aarch64" | "Linux arm64") target=aarch64-unknown-linux-gnu ;;
  *) target=x86_64-unknown-linux-gnu ;;
esac
release="$work/release"
pkg="speccoder-$version-$target"
mkdir -p "$release" "$work/$pkg"
printf '#!/bin/sh\necho "speccoder %s"\n' "$version" > "$work/$pkg/speccoder"
chmod +x "$work/$pkg/speccoder"
tar -czf "$release/$pkg.tar.gz" -C "$work" "$pkg"
sum() { if command -v sha256sum >/dev/null; then sha256sum "$1"; else shasum -a 256 "$1"; fi | awk '{print $1}'; }
echo "$(sum "$release/$pkg.tar.gz")  $pkg.tar.gz" > "$release/$pkg.tar.gz.sha256"

run() { SPECCODER_DOWNLOAD_URL="file://$release" SPECCODER_VERSION="$version" sh "$installer" "$@"; }

# Good archive.
run --to "$work/bin" 2>"$work/log" || fail "good archive: exit $? ($(cat "$work/log"))"
[ "$("$work/bin/speccoder" --version)" = "speccoder $version" ] || fail "installed binary has the wrong version"

# Tampered archive: exit 3, nothing installed.
echo garbage >> "$release/$pkg.tar.gz"
set +e
run --to "$work/bin2" 2>"$work/log"; code=$?
set -e
[ "$code" = 3 ] || fail "tampered archive: exit $code, want 3"
[ ! -e "$work/bin2/speccoder" ] || fail "tampered archive was installed"
grep -q "checksum mismatch" "$work/log" || fail "tampered archive: no mismatch message"

# Unsupported platform: exit 2.
set +e
SPECCODER_TEST_UNAME="Windows_NT x86_64" run --to "$work/bin3" 2>/dev/null; code=$?
set -e
[ "$code" = 2 ] || fail "unsupported platform: exit $code, want 2"

echo "test-installer: ok"
