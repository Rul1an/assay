#!/usr/bin/env bash
# Read and validate the single checked-in Assay Action consumer pin.
#
# The pin file is the only place the consumed Action commit lives; this is the
# only parser. Workflows must copy the 40-hex literal into `uses:`; Actions
# does not permit `uses: ${{ pin }}`.
#
# Outputs:
#   - stdout: the validated 40-hex commit
#
# Override the pin path with ASSAY_ACTION_PIN_FILE (tests only).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PIN_FILE="${ASSAY_ACTION_PIN_FILE:-${ROOT}/.github/assay-action-pin}"

if [[ ! -f "${PIN_FILE}" ]]; then
  echo "assay action pin missing: ${PIN_FILE}" >&2
  echo "assay action pin is malformed (want exactly one ^[0-9a-f]{40}$ line)" >&2
  exit 1
fi

pins=()
while IFS= read -r line || [[ -n "${line}" ]]; do
  line="${line//$'\r'/}"
  line="${line#"${line%%[![:space:]]*}"}"
  line="${line%"${line##*[![:space:]]}"}"
  [[ -z "${line}" ]] && continue
  pins+=("${line}")
done <"${PIN_FILE}"

if [[ "${#pins[@]}" -ne 1 ]] || [[ ! "${pins[0]}" =~ ^[0-9a-f]{40}$ ]]; then
  echo "assay action pin is malformed (want exactly one ^[0-9a-f]{40}$ line): ${PIN_FILE}" >&2
  echo "the pin lives in .github/assay-action-pin and is the only place to change it" >&2
  exit 1
fi

printf '%s\n' "${pins[0]}"
