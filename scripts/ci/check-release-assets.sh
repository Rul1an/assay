#!/usr/bin/env bash
set -euo pipefail

VERSION="${VERSION:?VERSION is required, for example v3.10.0}"
ASSETS_DIR="${ASSETS_DIR:-release}"
JQ_BIN="${JQ_BIN:-jq}"
REPO="${REPO:-Rul1an/assay}"

require_bin() {
  local bin="$1"
  if ! command -v "$bin" >/dev/null 2>&1; then
    echo "missing required binary: $bin" >&2
    exit 1
  fi
}

compute_sha256() {
  local file="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | awk '{print $1}'
  else
    shasum -a 256 "$file" | awk '{print $1}'
  fi
}

basename_from_checksum_line() {
  local line="$1"
  line="${line#* }"
  line="${line#"${line%%[![:space:]]*}"}"
  line="${line#\\*}"
  line="${line//$'\r'/}"
  basename "$line"
}

require_bin "$JQ_BIN"

if [[ ! "$VERSION" =~ ^v[0-9]+[.][0-9]+[.][0-9]+([-.+][0-9A-Za-z.-]+)?$ ]]; then
  echo "release VERSION must be a v-prefixed semver tag: $VERSION" >&2
  exit 1
fi

if [[ ! -d "$ASSETS_DIR" ]]; then
  echo "release assets directory not found: $ASSETS_DIR" >&2
  exit 1
fi

non_files="$(find "$ASSETS_DIR" -mindepth 1 -maxdepth 1 ! -type f -print | sort)"
if [[ -n "$non_files" ]]; then
  echo "release assets directory must contain only regular files" >&2
  printf '%s\n' "$non_files" | sed 's/^/  - /' >&2
  exit 1
fi

checksum_targets=(
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

plain_assets=(
  "server.json"
)

scratch_dir="$(mktemp -d)"
trap 'rm -rf "$scratch_dir"' EXIT
expected_files="$scratch_dir/expected-files.txt"
actual_files="$scratch_dir/actual-files.txt"

: >"$expected_files"
for asset in "${checksum_targets[@]}"; do
  printf '%s\n' "$asset" >>"$expected_files"
  printf '%s.sha256\n' "$asset" >>"$expected_files"
done
for asset in "${plain_assets[@]}"; do
  printf '%s\n' "$asset" >>"$expected_files"
done
sort -o "$expected_files" "$expected_files"

find "$ASSETS_DIR" -mindepth 1 -maxdepth 1 -type f -exec basename {} \; | sort >"$actual_files"
if ! diff -u "$expected_files" "$actual_files"; then
  echo "release asset set does not match the expected contract" >&2
  echo "expected exactly $(wc -l <"$expected_files" | tr -d ' ') files in $ASSETS_DIR" >&2
  exit 1
fi

for asset in "${checksum_targets[@]}"; do
  asset_path="$ASSETS_DIR/$asset"
  checksum_path="${asset_path}.sha256"

  if [[ ! -s "$asset_path" ]]; then
    echo "release asset is missing or empty: $asset" >&2
    exit 1
  fi
  if [[ ! -s "$checksum_path" ]]; then
    echo "release checksum is missing or empty: $(basename "$checksum_path")" >&2
    exit 1
  fi

  checksum_line="$(head -n 1 "$checksum_path")"
  expected_hash="$(awk '{print $1}' <<<"$checksum_line")"
  checksum_target="$(basename_from_checksum_line "$checksum_line")"
  actual_hash="$(compute_sha256 "$asset_path")"

  if [[ ! "$expected_hash" =~ ^[0-9a-f]{64}$ ]]; then
    echo "release checksum has invalid sha256 format: $(basename "$checksum_path")" >&2
    exit 1
  fi
  if [[ "$checksum_target" != "$asset" ]]; then
    echo "release checksum target mismatch in $(basename "$checksum_path"): expected $asset, got $checksum_target" >&2
    exit 1
  fi
  if [[ "$expected_hash" != "$actual_hash" ]]; then
    echo "release checksum mismatch for $asset" >&2
    exit 1
  fi
done

mcpb_asset="assay-mcp-server-${VERSION}-linux.mcpb"
mcpb_sha="$(awk '{print $1}' "$ASSETS_DIR/${mcpb_asset}.sha256")"
semver="${VERSION#v}"
mcpb_url="https://github.com/${REPO}/releases/download/${VERSION}/${mcpb_asset}"
# shellcheck disable=SC2016
if ! "$JQ_BIN" -e \
  --arg version "$semver" \
  --arg mcpb_asset "$mcpb_asset" \
  --arg mcpb_sha "$mcpb_sha" \
  --arg mcpb_url "$mcpb_url" \
  '
  .version == $version
  and .name == "io.github.Rul1an/assay-mcp-server"
  and .title == "Assay MCP Server"
  and .repository.url == "https://github.com/Rul1an/assay"
  and .repository.source == "github"
  and (.packages | length == 1)
  and .packages[0].version == $version
  and .packages[0].registryType == "mcpb"
  and .packages[0].identifier == $mcpb_url
  and .packages[0].fileSha256 == $mcpb_sha
  ' "$ASSETS_DIR/server.json" >/dev/null; then
  echo "release server.json does not match the generated MCPB asset contract" >&2
  exit 1
fi

{
  echo "## Release Asset Preflight"
  echo
  echo "- version: \`$VERSION\`"
  echo "- assets_dir: \`$ASSETS_DIR\`"
  echo "- file_count: \`$(wc -l <"$actual_files" | tr -d ' ')\`"
  echo
  echo "Validated exact release asset set, sha256 files, and MCP Registry server.json linkage."
} >>"${GITHUB_STEP_SUMMARY:-/dev/null}"

echo "release asset preflight passed for $VERSION ($(wc -l <"$actual_files" | tr -d ' ') files)"
