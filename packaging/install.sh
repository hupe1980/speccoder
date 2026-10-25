#!/bin/sh
# speccoder installer: downloads a release archive, checks its SHA-256 (and,
# with --verify-attestation, its build provenance), and installs the binary.
#
#   curl -LsSf https://github.com/hupe1980/speccoder/releases/latest/download/speccoder-installer.sh | sh
#   sh speccoder-installer.sh [--version <v>] [--to <dir>] [--verify-attestation]
#
# Environment: SPECCODER_VERSION, SPECCODER_INSTALL_DIR (default
# $HOME/.local/bin), SPECCODER_DOWNLOAD_URL (base URL, file:// allowed).
# Exit: 0 installed, 1 download or tool missing, 2 unsupported platform,
# 3 checksum mismatch, 4 attestation failed. Never uses sudo.
set -eu

repo="hupe1980/speccoder"
version="${SPECCODER_VERSION:-}"
dir="${SPECCODER_INSTALL_DIR:-${HOME:-}/.local/bin}"
attest=false

say() { printf 'speccoder-installer: %s\n' "$*" >&2; }
die() { code="$1"; shift; say "$*"; exit "$code"; }

usage() {
  sed -n '2,11s/^# \{0,1\}//p' "$0" 2>/dev/null || true
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --version) [ "$#" -ge 2 ] || die 1 "--version needs a value"; version="$2"; shift 2 ;;
    --version=*) version="${1#--version=}"; shift ;;
    --to) [ "$#" -ge 2 ] || die 1 "--to needs a directory"; dir="$2"; shift 2 ;;
    --to=*) dir="${1#--to=}"; shift ;;
    --verify-attestation) attest=true; shift ;;
    -h|--help) usage; exit 0 ;;
    *) die 1 "unknown option $1 (see --help)" ;;
  esac
done
version="${version#v}"

# Platform.
if [ -n "${SPECCODER_TEST_UNAME:-}" ]; then
  os="${SPECCODER_TEST_UNAME% *}"
  arch="${SPECCODER_TEST_UNAME#* }"
else
  os="$(uname -s)"
  arch="$(uname -m)"
fi
case "$os $arch" in
  "Darwin arm64" | "Darwin aarch64") target=aarch64-apple-darwin ;;
  "Darwin x86_64") die 2 "no speccoder build for Intel Macs; build it from source: cargo install speccoder --locked" ;;
  "Linux aarch64" | "Linux arm64") target=aarch64-unknown-linux-gnu ;;
  "Linux x86_64") target=x86_64-unknown-linux-gnu ;;
  *) die 2 "no speccoder build for $os $arch (supported: macOS on Apple silicon, Linux on x86_64 or arm64)" ;;
esac
if [ "$os" = Linux ] && [ -z "${SPECCODER_TEST_UNAME:-}" ] && ldd --version 2>&1 | grep -qi musl; then
  die 2 "musl-based Linux is not supported (the builds link glibc)"
fi

# Tools.
if command -v curl >/dev/null 2>&1; then
  fetch() { curl -fsSL "$1" -o "$2"; }
  latest() { curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/$repo/releases/latest"; }
elif command -v wget >/dev/null 2>&1; then
  fetch() { wget -q "$1" -O "$2"; }
  latest() { wget -q --max-redirect=5 -S -O /dev/null "https://github.com/$repo/releases/latest" 2>&1 | awk '/^  Location: /{l=$2} END{print l}'; }
else
  die 1 "needs curl or wget"
fi
if command -v sha256sum >/dev/null 2>&1; then
  sha256() { sha256sum "$1" | awk '{print $1}'; }
elif command -v shasum >/dev/null 2>&1; then
  sha256() { shasum -a 256 "$1" | awk '{print $1}'; }
else
  die 1 "needs sha256sum or shasum"
fi
command -v tar >/dev/null 2>&1 || die 1 "needs tar"

if [ -z "$version" ]; then
  url="$(latest)" || die 1 "cannot find the latest release"
  version="${url##*/tag/v}"
  case "$version" in
    "" | */*) die 1 "cannot find the latest release (got $url)" ;;
  esac
fi
base="${SPECCODER_DOWNLOAD_URL:-https://github.com/$repo/releases/download/v$version}"
archive="speccoder-$version-$target.tar.gz"

tmp="$(mktemp -d 2>/dev/null || mktemp -d -t speccoder)"
trap 'rm -rf "$tmp"' EXIT INT TERM

say "downloading speccoder $version for $target"
fetch "$base/$archive" "$tmp/$archive" || die 1 "cannot download $base/$archive"
fetch "$base/$archive.sha256" "$tmp/$archive.sha256" || die 1 "cannot download $base/$archive.sha256"

expected="$(awk '{print $1; exit}' "$tmp/$archive.sha256")"
actual="$(sha256 "$tmp/$archive")"
if [ "$expected" != "$actual" ]; then
  say "checksum mismatch for $archive"
  say "  expected $expected"
  say "  actual   $actual"
  die 3 "nothing installed"
fi

if [ "$attest" = true ]; then
  command -v gh >/dev/null 2>&1 || die 4 "--verify-attestation needs the GitHub CLI (gh)"
  gh attestation verify "$tmp/$archive" --repo "$repo" >/dev/null 2>&1 \
    || die 4 "the build provenance of $archive did not verify; nothing installed"
  say "build provenance verified"
fi

tar -xzf "$tmp/$archive" -C "$tmp" || die 1 "cannot unpack $archive"
binary="$tmp/speccoder-$version-$target/speccoder"
[ -f "$binary" ] || die 1 "$archive has no speccoder binary"

mkdir -p "$dir" || die 1 "cannot create $dir"
cp "$binary" "$dir/speccoder.tmp.$$" && chmod 755 "$dir/speccoder.tmp.$$" \
  && mv "$dir/speccoder.tmp.$$" "$dir/speccoder" || die 1 "cannot write $dir/speccoder"

installed="$("$dir/speccoder" --version 2>/dev/null || echo "speccoder $version")"
say "installed $installed to $dir/speccoder"
case ":${PATH:-}:" in
  *":$dir:"*) ;;
  *) say "add $dir to your PATH, e.g.: export PATH=\"$dir:\$PATH\"" ;;
esac
