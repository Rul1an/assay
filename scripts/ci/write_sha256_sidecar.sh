#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: write_sha256_sidecar.sh <asset-path>" >&2
  exit 2
fi

asset_path="$1"
if [[ ! -f "$asset_path" ]]; then
  echo "checksum asset is not a regular file: $asset_path" >&2
  exit 1
fi

if command -v sha256sum >/dev/null 2>&1; then
  digest="$(sha256sum "$asset_path" | awk '{print $1}')"
elif command -v shasum >/dev/null 2>&1; then
  digest="$(shasum -a 256 "$asset_path" | awk '{print $1}')"
else
  echo "sha256sum or shasum is required" >&2
  exit 1
fi

asset_name="${asset_path##*/}"
printf '%s  %s\n' "$digest" "$asset_name" >"${asset_path}.sha256"
