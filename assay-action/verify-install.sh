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

INSTALLED_OUTPUT="$("$BINARY" --version 2>/dev/null || true)"
INSTALLED_VERSION=""
if [[ "$INSTALLED_OUTPUT" != *$'\n'* &&
  "$INSTALLED_OUTPUT" != *$'\r'* &&
  "$INSTALLED_OUTPUT" =~ ^assay[[:space:]]+([0-9A-Za-z.+-]+)$ ]]; then
  INSTALLED_VERSION="${BASH_REMATCH[1]}"
fi
if [[ "$INSTALLED_VERSION" != "$EXPECTED_VERSION" ]]; then
  echo "::error::Assay installation verification failed: expected ${EXPECTED_VERSION}, got ${INSTALLED_VERSION:-unknown}"
  exit 1
fi

printf '%s\n' "$(dirname "$BINARY")" >>"$GITHUB_PATH"
"$BINARY" --version
echo "installed=true" >>"$GITHUB_OUTPUT"
