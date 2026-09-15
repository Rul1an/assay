#!/usr/bin/env bash
# A prerelease baseline licenses breaking changes; compare against a stable tag.
set -euo pipefail
ROOT="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
tag="$(git -C "$ROOT" tag --list 'v[0-9]*' --sort=-v:refname |
  awk '/^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$/ { if (!found) { print; found=1 } }')"
if [ -z "$tag" ]; then
  echo 'no stable vX.Y.Z release tag found; semver baseline is unavailable' >&2
  exit 1
fi
git -C "$ROOT" cat-file -e "${tag}^{commit}"
printf '%s\n' "$tag"
