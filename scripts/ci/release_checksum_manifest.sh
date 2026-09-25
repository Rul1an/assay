#!/usr/bin/env bash
# Write, verify, and optionally sign a sha256 manifest over every release asset.
# Signing requires cosign and is intended for the release job only.
set -euo pipefail

usage() {
  cat <<'EOF' >&2
usage:
  release_checksum_manifest.sh write --dir <assets-dir>
  release_checksum_manifest.sh verify --dir <assets-dir>
  release_checksum_manifest.sh check-contract --dir <assets-dir>
  release_checksum_manifest.sh sign --dir <assets-dir> --certificate-identity <url>
EOF
  exit 2
}

CERT_OIDC_ISSUER="${CERT_OIDC_ISSUER:-https://token.actions.githubusercontent.com}"
COSIGN_BIN="${COSIGN:-cosign}"
MANIFEST_NAME="checksums.txt"
SIGNATURE_NAME="checksums.txt.sigstore.json"

is_manifest_family() {
  case "$1" in
    "$MANIFEST_NAME" | "$SIGNATURE_NAME")
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

compute_sha256() {
  local file="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$file" | awk '{print $1}'
  else
    echo "sha256sum or shasum is required" >&2
    exit 1
  fi
}

list_payload_names() {
  local dir="$1"
  local name
  while IFS= read -r name; do
    if is_manifest_family "$name"; then
      continue
    fi
    printf '%s\n' "$name"
  done < <(find "$dir" -mindepth 1 -maxdepth 1 -type f -exec basename {} \; | LC_ALL=C sort)
}

parse_dir_args() {
  assets_dir=""
  certificate_identity=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --dir)
        [[ $# -ge 2 ]] || usage
        assets_dir="$2"
        shift 2
        ;;
      --certificate-identity)
        [[ $# -ge 2 ]] || usage
        certificate_identity="$2"
        shift 2
        ;;
      *)
        usage
        ;;
    esac
  done
  [[ -n "$assets_dir" ]] || usage
  [[ -d "$assets_dir" ]] || {
    echo "assets directory not found: $assets_dir" >&2
    exit 1
  }
}

manifest_path() {
  printf '%s/%s\n' "$assets_dir" "$MANIFEST_NAME"
}

signature_path() {
  printf '%s/%s\n' "$assets_dir" "$SIGNATURE_NAME"
}

cmd_write() {
  parse_dir_args "$@"
  local out names name digest
  out="$(manifest_path)"
  names="$(list_payload_names "$assets_dir")"
  if [[ -z "$names" ]]; then
    echo "no release assets to hash in $assets_dir" >&2
    exit 1
  fi
  : >"$out"
  while IFS= read -r name; do
    [[ -n "$name" ]] || continue
    digest="$(compute_sha256 "${assets_dir}/${name}")"
    printf '%s  %s\n' "$digest" "$name" >>"$out"
  done <<<"$names"
}

cmd_verify() {
  parse_dir_args "$@"
  local out line digest name actual
  out="$(manifest_path)"
  [[ -s "$out" ]] || {
    echo "checksums.txt is missing or empty" >&2
    exit 1
  }
  while IFS= read -r line || [[ -n "$line" ]]; do
    [[ -n "$line" ]] || continue
    digest="${line:0:64}"
    if [[ "${line:64:2}" != "  " ]]; then
      echo "checksums.txt line is not sha256  basename: $line" >&2
      exit 1
    fi
    name="${line:66}"
    if [[ ! "$digest" =~ ^[0-9a-f]{64}$ ]] || [[ -z "$name" ]] || [[ "$name" == */* ]]; then
      echo "checksums.txt line is not sha256  basename: $line" >&2
      exit 1
    fi
    if [[ ! -f "${assets_dir}/${name}" ]]; then
      echo "checksums.txt names a missing asset: $name" >&2
      exit 1
    fi
    actual="$(compute_sha256 "${assets_dir}/${name}")"
    if [[ "$actual" != "$digest" ]]; then
      echo "checksums.txt hash mismatch: $name" >&2
      exit 1
    fi
  done <"$out"
}

cmd_check_contract() {
  parse_dir_args "$@"
  local out listed actual name
  out="$(manifest_path)"
  [[ -s "$out" ]] || {
    echo "checksums.txt is missing or empty" >&2
    exit 1
  }
  listed="$(mktemp)"
  actual="$(mktemp)"
  trap 'rm -f "$listed" "$actual"' RETURN
  awk '{print $2}' "$out" | LC_ALL=C sort >"$listed"
  list_payload_names "$assets_dir" >"$actual"
  if ! diff -u "$listed" "$actual"; then
    echo "checksums.txt does not match the published asset set" >&2
    comm -13 "$listed" "$actual" | while IFS= read -r name; do
      [[ -n "$name" ]] || continue
      echo "checksums.txt omits published asset: $name" >&2
    done
    comm -23 "$listed" "$actual" | while IFS= read -r name; do
      [[ -n "$name" ]] || continue
      echo "checksums.txt lists unpublished asset: $name" >&2
    done
    exit 1
  fi
}

cmd_sign() {
  parse_dir_args "$@"
  [[ -n "$certificate_identity" ]] || usage
  local out bundle
  out="$(manifest_path)"
  bundle="$(signature_path)"
  [[ -s "$out" ]] || {
    echo "checksums.txt is missing or empty; write it before signing" >&2
    exit 1
  }
  if ! command -v "$COSIGN_BIN" >/dev/null 2>&1; then
    echo "cosign is required to sign checksums.txt; install it in the release job" >&2
    exit 1
  fi
  "$COSIGN_BIN" sign-blob --yes --bundle "$bundle" "$out"
  if ! "$COSIGN_BIN" verify-blob \
    --bundle "$bundle" \
    --certificate-identity "$certificate_identity" \
    --certificate-oidc-issuer "$CERT_OIDC_ISSUER" \
    "$out"; then
    echo "signed checksums.txt did not verify against certificate identity ${certificate_identity}" >&2
    echo "issuer ${CERT_OIDC_ISSUER}" >&2
    exit 1
  fi
}

[[ $# -ge 1 ]] || usage
command="$1"
shift
case "$command" in
  write)
    cmd_write "$@"
    ;;
  verify)
    cmd_verify "$@"
    ;;
  check-contract)
    cmd_check_contract "$@"
    ;;
  sign)
    cmd_sign "$@"
    ;;
  *)
    usage
    ;;
esac
