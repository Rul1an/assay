#!/usr/bin/env bash
set -euo pipefail

BINARY="${1:-}"
EXPECTED_VERSION="${2:-}"

if [[ ! -x "$BINARY" ]]; then
  echo "::error::Assay installation verification failed"
  exit 1
fi
if [[ -z "$EXPECTED_VERSION" || "$EXPECTED_VERSION" == *$'\n'* || "$EXPECTED_VERSION" == *$'\r'* ]]; then
  echo "::error::Assay installation verification received an invalid expected version"
  exit 1
fi

printf '%s\n' "$(dirname "$BINARY")" >>"$GITHUB_PATH"
INSTALLED_VERSION="$("$BINARY" --version | awk '{print $2}')"
if [[ "$INSTALLED_VERSION" != "$EXPECTED_VERSION" ]]; then
  echo "::error::Assay installation verification failed: expected ${EXPECTED_VERSION}, got ${INSTALLED_VERSION:-unknown}"
  exit 1
fi

"$BINARY" --version
echo "installed=true" >>"$GITHUB_OUTPUT"
