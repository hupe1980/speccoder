#!/usr/bin/env bash
# Publish the crates to crates.io in dependency order (release.yml, job
# crates; or `just publish` by the maintainer). Resumable: a crate already on
# crates.io at this version is skipped.
#
#   scripts/release/publish.sh speccoder-core speccoder-speckit …
set -euo pipefail
[ "$#" -gt 0 ] || { echo "usage: publish.sh <crate>…" >&2; exit 2; }
version="$(sed -n '/^\[workspace.package\]/,/^\[/{s/^version = "\(.*\)"/\1/p;}' Cargo.toml | head -1)"
if [[ "$version" == *-* ]]; then
  echo "publish: $version is a pre-release; crates are published for releases only" >&2
  exit 1
fi
if [ -n "$(git status --porcelain)" ]; then
  echo "publish: the working tree is not clean" >&2
  exit 1
fi
if [ "$(git tag --points-at HEAD | grep -cx "v$version" || true)" != 1 ]; then
  echo "publish: HEAD is not tagged v$version" >&2
  exit 1
fi

# The sparse index path of a crate name (crates.io's layout).
index_path() {
  local n="$1"
  case ${#n} in
    1) echo "1/$n" ;;
    2) echo "2/$n" ;;
    3) echo "3/${n:0:1}/$n" ;;
    *) echo "${n:0:2}/${n:2:2}/$n" ;;
  esac
}

for crate in "$@"; do
  if curl -fsSL "https://index.crates.io/$(index_path "$crate")" 2>/dev/null \
      | jq -e --arg v "$version" 'select(.vers == $v)' >/dev/null; then
    echo "publish: $crate $version is on crates.io; skipped"
    continue
  fi
  echo "publish: $crate $version"
  cargo publish -p "$crate" --locked
done
