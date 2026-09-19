#!/usr/bin/env bash
# Network-isolated consumer verify of a signed checksums.txt, then one archive hash.
# Bootstrap (online initialize) and isolated verify-blob are separate phases.
set -euo pipefail

usage() {
  cat <<'EOF' >&2
usage:
  verify_consumer_checksum_manifest.sh \
    --assets-dir <path> \
    --archive <filename> \
    --certificate-identity <url> \
    --certificate-oidc-issuer <url> \
    [--cosign-image <name@sha256:digest>]
EOF
  exit 2
}

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
PIN_FILE="${CONSUMER_COSIGN_IMAGE_FILE:-${REPO_ROOT}/.github/cosign-image}"
TRUSTED_ROOT_REL='tuf-cache/tuf-repo-cdn.sigstore.dev/targets/trusted_root.json'

assets_dir=""
archive=""
certificate_identity=""
certificate_oidc_issuer=""
cosign_image=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --assets-dir)
      [[ $# -ge 2 ]] || usage
      assets_dir="$2"
      shift 2
      ;;
    --archive)
      [[ $# -ge 2 ]] || usage
      archive="$2"
      shift 2
      ;;
    --certificate-identity)
      [[ $# -ge 2 ]] || usage
      certificate_identity="$2"
      shift 2
      ;;
    --certificate-oidc-issuer)
      [[ $# -ge 2 ]] || usage
      certificate_oidc_issuer="$2"
      shift 2
      ;;
    --cosign-image)
      [[ $# -ge 2 ]] || usage
      cosign_image="$2"
      shift 2
      ;;
    *)
      usage
      ;;
  esac
done

[[ -n "$assets_dir" && -n "$archive" && -n "$certificate_identity" && -n "$certificate_oidc_issuer" ]] || usage

reject() {
  echo "$1" >&2
  exit 1
}

looks_like_flag() {
  [[ "$1" == -* ]]
}

has_unsafe_path_component() {
  local path="$1"
  local rest="$path"
  local part
  while [[ -n "$rest" ]]; do
    part="${rest%%/*}"
    if [[ "$part" == ".." || "$part" == "." ]]; then
      return 0
    fi
    if [[ "$rest" == */* ]]; then
      rest="${rest#*/}"
    else
      rest=""
    fi
  done
  return 1
}

read_cosign_pin() {
  local pin_file="$1"
  [[ -f "$pin_file" ]] || reject "cosign image pin missing: $pin_file"
  local refs=()
  local line
  while IFS= read -r line || [[ -n "$line" ]]; do
    line="${line//$'\r'/}"
    line="${line#"${line%%[![:space:]]*}"}"
    line="${line%"${line##*[![:space:]]}"}"
    [[ -z "$line" ]] && continue
    refs+=("$line")
  done <"$pin_file"
  [[ "${#refs[@]}" -eq 1 ]] || reject "cosign image pin must be a single line: $pin_file"
  printf '%s\n' "${refs[0]}"
}

looks_like_flag "$assets_dir" && reject "assets-dir must not be a flag"
looks_like_flag "$archive" && reject "archive must not be a flag"
looks_like_flag "$certificate_identity" && reject "certificate-identity must not be a flag"
looks_like_flag "$certificate_oidc_issuer" && reject "certificate-oidc-issuer must not be a flag"
if [[ -n "$cosign_image" ]] && looks_like_flag "$cosign_image"; then
  reject "cosign-image must not be a flag"
fi

[[ "$assets_dir" != *:* ]] || reject "assets-dir must not contain a colon"
[[ "$assets_dir" != *$'\n'* && "$archive" != *$'\n'* ]] || reject "paths must not contain a newline"
has_unsafe_path_component "$assets_dir" && reject "assets-dir must not contain . or .. components"
has_unsafe_path_component "$archive" && reject "archive must be a basename without . or .."

[[ "$archive" != */* ]] || reject "archive must be a basename"
[[ "$archive" =~ ^[A-Za-z0-9._+-]+$ ]] || reject "archive contains characters that are not safe as a basename"

[[ -d "$assets_dir" ]] || reject "assets directory not found: $assets_dir"
[[ -f "${assets_dir}/checksums.txt" ]] || reject "checksums.txt is missing"
[[ -f "${assets_dir}/checksums.txt.sigstore.json" ]] || reject "checksums.txt.sigstore.json is missing"
[[ -f "${assets_dir}/${archive}" ]] || reject "selected archive is missing: $archive"

[[ "$certificate_identity" =~ ^https://github.com/[^/]+/[^/]+/.github/workflows/release.yml@refs/tags/.+$ ]] \
  || reject "certificate-identity must be the release workflow at refs/tags/<tag>"
[[ "$certificate_identity" != *'@refs/heads/'* ]] \
  || reject "certificate-identity must not accept a branch ref"
[[ "$certificate_oidc_issuer" == https://token.actions.githubusercontent.com ]] \
  || reject "certificate-oidc-issuer must be https://token.actions.githubusercontent.com"

if [[ -z "$cosign_image" ]]; then
  cosign_image="$(read_cosign_pin "$PIN_FILE")"
fi
[[ "$cosign_image" =~ ^[A-Za-z0-9._/-]+@sha256:[0-9a-f]{64}$ ]] \
  || reject "cosign-image must be name@sha256:<64 lowercase hex>"

command -v docker >/dev/null 2>&1 || reject "docker is required for network-isolated consumer verify"

hash_check() {
  local line="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    printf '%s\n' "$line" | (cd "$assets_dir" && sha256sum -c -)
  elif command -v shasum >/dev/null 2>&1; then
    printf '%s\n' "$line" | (cd "$assets_dir" && shasum -a 256 -c -)
  else
    reject "sha256sum or shasum is required"
  fi
}

scratch="$(mktemp -d "${TMPDIR:-/tmp}/assay-consumer-tuf.XXXXXX")"
cleanup() {
  if [[ -n "${scratch:-}" && -d "$scratch" ]]; then
    case "$scratch" in
      "${TMPDIR:-/tmp}"/assay-consumer-tuf.*)
        rm -rf -- "$scratch"
        ;;
    esac
  fi
}
trap cleanup EXIT

mkdir -p "${scratch}/tuf-cache"

docker run --rm \
  -e TUF_ROOT=/scratch/tuf-cache \
  -v "${scratch}:/scratch" \
  "$cosign_image" \
  initialize

trusted_root="${scratch}/${TRUSTED_ROOT_REL}"
[[ -f "$trusted_root" ]] || reject "bootstrap did not produce a modern trusted_root.json"

docker run --rm --network=none \
  -v "${assets_dir}:/assets:ro" \
  -v "${trusted_root}:/trusted_root.json:ro" \
  "$cosign_image" \
  verify-blob \
    --bundle /assets/checksums.txt.sigstore.json \
    --trusted-root /trusted_root.json \
    --certificate-identity "$certificate_identity" \
    --certificate-oidc-issuer "$certificate_oidc_issuer" \
    /assets/checksums.txt

selected_line=""
match_count=0
while IFS= read -r line || [[ -n "$line" ]]; do
  [[ -n "$line" ]] || continue
  name="${line#*  }"
  if [[ "$name" == "$archive" ]]; then
    match_count=$((match_count + 1))
    selected_line="$line"
  fi
done <"${assets_dir}/checksums.txt"

if [[ "$match_count" -eq 0 ]]; then
  echo "checksums.txt does not name ${archive}" >&2
  exit 1
fi
if [[ "$match_count" -gt 1 ]]; then
  echo "checksums.txt has a duplicate entry for ${archive}" >&2
  exit 1
fi

hash_check "$selected_line"
