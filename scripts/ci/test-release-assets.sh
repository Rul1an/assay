#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
CHECK_SCRIPT="${REPO_ROOT}/scripts/ci/check-release-assets.sh"
VERSION="v9.9.9"
SEMVER="${VERSION#v}"

compute_sha256() {
  local file="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | awk '{print $1}'
  else
    shasum -a 256 "$file" | awk '{print $1}'
  fi
}

write_asset() {
  local assets_dir="$1"
  local name="$2"
  printf 'fixture payload for %s\n' "$name" >"${assets_dir}/${name}"
  printf '%s  %s\n' "$(compute_sha256 "${assets_dir}/${name}")" "$name" >"${assets_dir}/${name}.sha256"
}

write_server_json() {
  local assets_dir="$1"
  local sha="$2"
  cat >"${assets_dir}/server.json" <<EOF
{
  "\$schema": "https://static.modelcontextprotocol.io/schemas/2025-12-11/server.schema.json",
  "name": "io.github.Rul1an/assay-mcp-server",
  "title": "Assay MCP Server",
  "repository": {
    "url": "https://github.com/Rul1an/assay",
    "source": "github"
  },
  "version": "${SEMVER}",
  "packages": [
    {
      "registryType": "mcpb",
      "identifier": "https://github.com/Rul1an/assay/releases/download/${VERSION}/assay-mcp-server-${VERSION}-linux.mcpb",
      "version": "${SEMVER}",
      "fileSha256": "${sha}",
      "transport": {
        "type": "stdio"
      }
    }
  ]
}
EOF
}

build_valid_assets() {
  local assets_dir="$1"
  mkdir -p "$assets_dir"
  local targets=(
    "assay-${VERSION}-x86_64-unknown-linux-gnu.tar.gz"
    "assay-${VERSION}-aarch64-unknown-linux-gnu.tar.gz"
    "assay-${VERSION}-x86_64-apple-darwin.tar.gz"
    "assay-${VERSION}-aarch64-apple-darwin.tar.gz"
    "assay-${VERSION}-x86_64-pc-windows-msvc.zip"
    "assay-mcp-server-${VERSION}-x86_64-unknown-linux-gnu.tar.gz"
    "assay-mcp-server-${VERSION}-aarch64-unknown-linux-gnu.tar.gz"
    "assay-mcp-server-${VERSION}-linux.mcpb"
    "assay-${VERSION}-sbom-cyclonedx.tar.gz"
    "assay-${VERSION}-release-provenance.json"
    "assay-${VERSION}-release-proof-kit.tar.gz"
  )
  for target in "${targets[@]}"; do
    write_asset "$assets_dir" "$target"
  done
  write_server_json "$assets_dir" "$(compute_sha256 "${assets_dir}/assay-mcp-server-${VERSION}-linux.mcpb")"
}

expect_pass() {
  local assets_dir="$1"
  VERSION="$VERSION" ASSETS_DIR="$assets_dir" bash "$CHECK_SCRIPT" >/dev/null
}

expect_fail() {
  local name="$1"
  local assets_dir="$2"
  local safe_name
  safe_name="$(printf '%s' "$name" | tr -c 'A-Za-z0-9_.-' '_')"
  local out_file="${tmp_root}/${safe_name}.out"
  local err_file="${tmp_root}/${safe_name}.err"
  if VERSION="$VERSION" ASSETS_DIR="$assets_dir" bash "$CHECK_SCRIPT" >"$out_file" 2>"$err_file"; then
    echo "expected failure for ${name}, but preflight passed" >&2
    exit 1
  fi
}

tmp_root="$(mktemp -d)"
trap 'rm -rf "$tmp_root"' EXIT

valid_dir="${tmp_root}/valid"
build_valid_assets "$valid_dir"
expect_pass "$valid_dir"

crlf_checksum_dir="${tmp_root}/crlf-checksum"
cp -R "$valid_dir" "$crlf_checksum_dir"
crlf_target="assay-${VERSION}-x86_64-pc-windows-msvc.zip"
printf '%s  %s\r\n' \
  "$(compute_sha256 "${crlf_checksum_dir}/${crlf_target}")" \
  "$crlf_target" >"${crlf_checksum_dir}/${crlf_target}.sha256"
expect_pass "$crlf_checksum_dir"

embedded_cr_target_dir="${tmp_root}/embedded-cr-target"
cp -R "$valid_dir" "$embedded_cr_target_dir"
printf '%s  %s\r%s\n' \
  "$(compute_sha256 "${embedded_cr_target_dir}/${crlf_target}")" \
  "assay-${VERSION}-x86_64-pc-windows-msvc" \
  ".zip" >"${embedded_cr_target_dir}/${crlf_target}.sha256"
expect_fail "embedded carriage return in checksum target" "$embedded_cr_target_dir"

missing_checksum_dir="${tmp_root}/missing-checksum"
cp -R "$valid_dir" "$missing_checksum_dir"
rm "${missing_checksum_dir}/assay-${VERSION}-x86_64-unknown-linux-gnu.tar.gz.sha256"
expect_fail "missing checksum" "$missing_checksum_dir"

empty_checksum_dir="${tmp_root}/empty-checksum"
cp -R "$valid_dir" "$empty_checksum_dir"
: >"${empty_checksum_dir}/assay-${VERSION}-x86_64-unknown-linux-gnu.tar.gz.sha256"
expect_fail "empty checksum" "$empty_checksum_dir"

checksum_mismatch_dir="${tmp_root}/checksum-mismatch"
cp -R "$valid_dir" "$checksum_mismatch_dir"
printf 'tampered\n' >>"${checksum_mismatch_dir}/assay-${VERSION}-x86_64-unknown-linux-gnu.tar.gz"
expect_fail "checksum mismatch" "$checksum_mismatch_dir"

unexpected_file_dir="${tmp_root}/unexpected-file"
cp -R "$valid_dir" "$unexpected_file_dir"
printf 'surprise\n' >"${unexpected_file_dir}/assay-${VERSION}-extra.tar.gz"
expect_fail "unexpected file" "$unexpected_file_dir"

server_mismatch_dir="${tmp_root}/server-mismatch"
cp -R "$valid_dir" "$server_mismatch_dir"
write_server_json "$server_mismatch_dir" "0000000000000000000000000000000000000000000000000000000000000000"
expect_fail "server.json sha mismatch" "$server_mismatch_dir"

echo "release asset preflight tests passed"
