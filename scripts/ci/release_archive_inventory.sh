#!/usr/bin/env bash
set -euo pipefail

assets_dir="${1:-${ASSETS_DIR:-}}"
if [ -z "${assets_dir}" ]; then
  echo "usage: release_archive_inventory.sh <assets-dir>" >&2
  exit 1
fi

find "${assets_dir}" -maxdepth 1 -type f \( -name '*.tar.gz' -o -name '*.zip' \) -print | sort
