#!/usr/bin/env bash
# Shared release asset and installability truth. Callers may source this file;
# it must not mutate shell options or global state.

release_normalize_version() {
  local version="${1#v}"
  [[ "$version" =~ ^[0-9]+[.][0-9]+[.][0-9]+([-.+][0-9A-Za-z.-]+)?$ ]] || return 1
  printf 'v%s\n' "$version"
}

release_installability_matrix() {
  local tag
  tag="$(release_normalize_version "$1")" || return 1

  printf '%s\t%s\t%s\t%s\n' \
    assay x86_64-unknown-linux-gnu installer "assay-${tag}-x86_64-unknown-linux-gnu.tar.gz" \
    assay aarch64-unknown-linux-gnu installer "assay-${tag}-aarch64-unknown-linux-gnu.tar.gz" \
    assay x86_64-apple-darwin installer "assay-${tag}-x86_64-apple-darwin.tar.gz" \
    assay aarch64-apple-darwin installer "assay-${tag}-aarch64-apple-darwin.tar.gz" \
    assay x86_64-pc-windows-msvc installer "assay-${tag}-x86_64-pc-windows-msvc.zip" \
    assay-mcp-server x86_64-unknown-linux-gnu manual_step "assay-mcp-server-${tag}-x86_64-unknown-linux-gnu.tar.gz" \
    assay-mcp-server aarch64-unknown-linux-gnu manual_step "assay-mcp-server-${tag}-aarch64-unknown-linux-gnu.tar.gz" \
    assay-mcp-server x86_64-apple-darwin unsupported - \
    assay-mcp-server aarch64-apple-darwin unsupported - \
    assay-mcp-server x86_64-pc-windows-msvc unsupported -
}

release_checksum_targets() {
  local tag
  tag="$(release_normalize_version "$1")" || return 1

  release_installability_matrix "$tag" | while IFS=$'\t' read -r _product _target status asset; do
    if [[ "$status" != unsupported ]]; then
      printf '%s\n' "$asset"
    fi
  done
  printf '%s\n' \
    "assay-mcp-server-${tag}-linux.mcpb" \
    "assay-${tag}-sbom-cyclonedx.tar.gz" \
    "assay-${tag}-release-provenance.json" \
    "assay-${tag}-release-proof-kit.tar.gz"
}

release_plain_assets() {
  printf '%s\n' server.json
}

release_expected_assets() {
  local asset
  while IFS= read -r asset; do
    printf '%s\n%s.sha256\n' "$asset" "$asset"
  done < <(release_checksum_targets "$1")
  release_plain_assets
}

release_installability_markdown() {
  local product target status asset
  printf '%s\n' \
    '| Component | Target | Install status | Release asset |' \
    '| --- | --- | --- | --- |'
  while IFS=$'\t' read -r product target status asset; do
    # shellcheck disable=SC2016
    printf '| `%s` | `%s` | `%s` | `%s` |\n' "$product" "$target" "$status" "$asset"
  done < <(release_installability_matrix "$1")
}
