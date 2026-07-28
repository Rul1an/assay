#!/usr/bin/env bash
set -euo pipefail

: "${ASSAY_PUBLIC_MSRV:?ASSAY_PUBLIC_MSRV must be set}"
expected_toolchain="$ASSAY_PUBLIC_MSRV"
expected_metadata="${expected_toolchain%.0}"
public_package_file="$(mktemp)"
trap 'rm -f "$public_package_file"' EXIT

command -v cargo >/dev/null 2>&1 || { echo "cargo missing" >&2; exit 1; }
command -v python3 >/dev/null 2>&1 || { echo "python3 missing" >&2; exit 1; }
command -v rustup >/dev/null 2>&1 || { echo "rustup missing" >&2; exit 1; }

echo "public-msrv-toolchain=${expected_toolchain}"
scripts/ci/check-public-crate-policy.sh
cargo metadata --locked --format-version 1 |
  python3 scripts/ci/check_msrv_metadata.py "$expected_metadata" > "$public_package_file"

if [[ "${ASSAY_MSRV_METADATA_ONLY:-0}" == "1" ]]; then
  exit 0
fi

rustc_version="$(rustup run "$expected_toolchain" rustc --version)"
if [[ "$rustc_version" != "rustc ${expected_toolchain} "* ]]; then
  echo "expected rustc ${expected_toolchain}, got: ${rustc_version}" >&2
  exit 1
fi

package_args=()
while IFS= read -r crate; do
  package_args+=("-p" "$crate")
done < "$public_package_file"

rustup run "$expected_toolchain" cargo check \
  --locked \
  --all-targets \
  "${package_args[@]}"

echo "public-msrv-check=passed"
