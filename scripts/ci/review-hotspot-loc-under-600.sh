#!/usr/bin/env bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

threshold="${HOTSPOT_LOC_THRESHOLD:-600}"

violations="$(
  find crates -name '*.rs' -type f \
    ! -path '*/target/*' \
    ! -name 'vmlinux.rs' \
    -print0 \
    | xargs -0 wc -l 2>/dev/null \
    | awk -v threshold="${threshold}" '$2 != "total" && $1 >= threshold { printf "%s %s\n", $1, $2 }' \
    | sort -rn
)"

if [[ -n "${violations}" ]]; then
  echo "FAIL: handwritten Rust files at or above ${threshold} LOC:" >&2
  printf '%s\n' "${violations}" >&2
  exit 1
fi

echo "PASS: no handwritten Rust files at or above ${threshold} LOC"
