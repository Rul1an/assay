#!/usr/bin/env bash
# Print the pre-commit version CI installs: the one pin in .github/workflows/kernel-matrix.yml
# (`pip install pre-commit==X`). Every installer of pre-commit asks this reader, so the number has
# one place. A missing or ambiguous pin prints no version and fails.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
source_file="${ROOT}/.github/workflows/kernel-matrix.yml"
pins="$(grep -oE 'pip install pre-commit==[0-9][0-9.]*' "${source_file}" | sed 's/^pip install pre-commit==//' | sort -u)" || true
count="$(printf '%s' "${pins}" | grep -c . || true)"
if [[ "${count}" != 1 ]]; then
  echo "read-pre-commit-pin: ${source_file#"${ROOT}"/} must pin pre-commit==VERSION exactly once, found ${count} distinct pin(s)" >&2
  exit 1
fi
printf '%s\n' "${pins}"
