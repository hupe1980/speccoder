#!/usr/bin/env bash
# Print the body of `## [<version>]` in CHANGELOG.md (release notes).
#
#   scripts/release/changelog-section.sh 0.1.0
#
# A pre-release version (with `-`) without a section of its own prints
# `[Unreleased]`. Exits 1 when the section is missing or empty.
set -euo pipefail
version="${1:?usage: changelog-section.sh <version>}"
file="${CHANGELOG:-CHANGELOG.md}"

section() {
  awk -v want="$1" '
    /^## \[/ {
      inside = (index($0, "## [" want "]") == 1)
      next
    }
    /^\[[^]]+\]: / { inside = 0 }
    inside { print }
  ' "$file" | sed -e '/./,$!d' | awk '{ lines[NR] = $0 } END { n = NR; while (n > 0 && lines[n] ~ /^[[:space:]]*$/) n--; for (i = 1; i <= n; i++) print lines[i] }'
}

body="$(section "$version")"
if [ -z "$body" ] && [[ "$version" == *-* ]]; then
  body="$(section Unreleased)"
fi
if [ -z "$body" ]; then
  echo "no CHANGELOG section for $version" >&2
  exit 1
fi
printf '%s\n' "$body"
