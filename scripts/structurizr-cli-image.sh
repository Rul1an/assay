#!/usr/bin/env bash
# Read and validate the single checked-in Structurizr CLI image pin.
#
# scripts/structurizr-validate.sh, scripts/structurizr-export.sh, and the
# structurizr-validate workflow all obtain the image from here. The pin file is
# the only place the digest literal lives; this is the only parser.
#
# Override the pin path with STRUCTURIZR_CLI_IMAGE_FILE (tests only).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PIN_FILE="${STRUCTURIZR_CLI_IMAGE_FILE:-${ROOT}/.github/structurizr-cli-image}"

if [[ ! -f "${PIN_FILE}" ]]; then
  echo "structurizr CLI image pin missing: ${PIN_FILE}" >&2
  exit 1
fi

refs=()
while IFS= read -r line || [[ -n "${line}" ]]; do
  line="${line//$'\r'/}"
  line="${line#"${line%%[![:space:]]*}"}"
  line="${line%"${line##*[![:space:]]}"}"
  [[ -z "${line}" ]] && continue
  refs+=("${line}")
done <"${PIN_FILE}"

if [[ "${#refs[@]}" -eq 0 ]]; then
  echo "structurizr CLI image pin is empty: ${PIN_FILE}" >&2
  exit 1
fi

if [[ "${#refs[@]}" -ne 1 ]]; then
  echo "structurizr CLI image pin must be a single line: ${PIN_FILE}" >&2
  exit 1
fi

image="${refs[0]}"

# Exactly structurizr/cli at a full sha256 digest. Refuse :latest, tags, short
# digests, and any other repository (a valid digest under a foreign name is still
# a supply-chain bypass).
if [[ ! "${image}" =~ ^structurizr/cli@sha256:[0-9a-f]{64}$ ]]; then
  echo "structurizr CLI image pin is malformed (want structurizr/cli@sha256:<64 lowercase hex>): '${image}'" >&2
  echo "the pin lives in .github/structurizr-cli-image and is the only place to change it" >&2
  exit 1
fi

printf '%s\n' "${image}"
