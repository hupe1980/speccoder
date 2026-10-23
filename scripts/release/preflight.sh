#!/usr/bin/env bash
# Checks a release tag before anything is built (release.yml, job preflight).
#
#   scripts/release/preflight.sh v0.1.0
#
# The tag is `v<semver>`, equals the workspace version, points at a commit
# on origin/main, and the version has CHANGELOG notes. Prints `version=`
# and `prerelease=` lines for $GITHUB_OUTPUT.
set -euo pipefail
tag="${1:?usage: preflight.sh <tag>}"
here="$(cd "$(dirname "$0")" && pwd)"
main="${PREFLIGHT_MAIN:-origin/main}"

semver='^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$'
if ! [[ "$tag" =~ $semver ]]; then
  echo "preflight: $tag is not v<semver>" >&2
  exit 1
fi
version="${tag#v}"

workspace="$(cargo metadata --format-version 1 --no-deps --locked \
  | jq -r '.packages[] | select(.name == "speccoder") | .version')"
if [ "$workspace" != "$version" ]; then
  echo "preflight: tag $tag but the workspace version is $workspace (run just bump $version)" >&2
  exit 1
fi

if ! git merge-base --is-ancestor HEAD "$main"; then
  echo "preflight: $tag is not on $main; release only from main" >&2
  exit 1
fi

if ! "$here/changelog-section.sh" "$version" >/dev/null; then
  echo "preflight: CHANGELOG.md has no notes for $version" >&2
  exit 1
fi

prerelease=false
[[ "$version" == *-* ]] && prerelease=true
echo "version=$version"
echo "prerelease=$prerelease"
