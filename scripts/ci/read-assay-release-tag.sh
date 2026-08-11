#!/usr/bin/env bash
# Read and validate the single checked-in Assay release tag pin.
#
# Both CI consumers (assay.yml local composite input and assay-security.yml
# install env) must call this script. The pin file is the only place the tag
# literal lives; this is the only parser.
#
# Outputs:
#   - stdout: the validated tag (e.g. v1.2.3)
#   - when GITHUB_OUTPUT is set: version=<tag>
#
# Override the pin path with ASSAY_RELEASE_TAG_FILE (tests only).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PIN_FILE="${ASSAY_RELEASE_TAG_FILE:-${ROOT}/.github/assay-release-tag}"

if [[ ! -f "${PIN_FILE}" ]]; then
  echo "assay release tag pin missing: ${PIN_FILE}" >&2
  exit 1
fi

# Collect non-empty lines. Command substitution would strip trailing newlines and
# hide a second line, so read line-by-line instead.
tags=()
while IFS= read -r line || [[ -n "${line}" ]]; do
  line="${line//$'\r'/}"
  # trim leading/trailing whitespace
  line="${line#"${line%%[![:space:]]*}"}"
  line="${line%"${line##*[![:space:]]}"}"
  [[ -z "${line}" ]] && continue
  tags+=("${line}")
done <"${PIN_FILE}"

if [[ "${#tags[@]}" -eq 0 ]]; then
  echo "assay release tag pin is empty: ${PIN_FILE}" >&2
  exit 1
fi

if [[ "${#tags[@]}" -ne 1 ]]; then
  echo "assay release tag pin must be a single line: ${PIN_FILE}" >&2
  exit 1
fi

tag="${tags[0]}"

# Stable software tags only. Refuse "latest", majors, and incomplete semver.
if [[ ! "${tag}" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "assay release tag pin is malformed (want vMAJOR.MINOR.PATCH): '${tag}'" >&2
  echo "the pin lives in .github/assay-release-tag and is the only place to change it" >&2
  exit 1
fi

printf '%s\n' "${tag}"

if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  printf 'version=%s\n' "${tag}" >>"${GITHUB_OUTPUT}"
fi
