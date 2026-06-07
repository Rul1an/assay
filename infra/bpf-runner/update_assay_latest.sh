#!/usr/bin/env bash
set -euo pipefail

REPO="${ASSAY_REPO:-Rul1an/assay}"
INSTALL_DIR="${ASSAY_INSTALL_DIR:-/usr/local/bin}"
INSTALL_OWNER="${ASSAY_INSTALL_OWNER:-root:root}"
COMPAT_SYMLINKS="${ASSAY_COMPAT_SYMLINKS:-/home/ubuntu/.cargo/bin/assay /home/github-runner/.cargo/bin/assay}"

case "$(uname -m)" in
  aarch64|arm64)
    TARGET="aarch64-unknown-linux-gnu"
    ;;
  x86_64|amd64)
    TARGET="x86_64-unknown-linux-gnu"
    ;;
  *)
    echo "unsupported architecture: $(uname -m)" >&2
    exit 1
    ;;
esac

if ! command -v curl >/dev/null 2>&1; then
  echo "curl is required to update assay" >&2
  exit 1
fi
if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required to update assay" >&2
  exit 1
fi

api_url="https://api.github.com/repos/${REPO}/releases/latest"
tag="$(curl -fsSL "$api_url" | jq -r '.tag_name // empty')"
if [[ -z "$tag" || "$tag" == "null" ]]; then
  echo "could not determine latest release tag for ${REPO}" >&2
  exit 1
fi

ensure_compat_symlinks() {
  for link_path in $COMPAT_SYMLINKS; do
    if [[ "$link_path" == "${INSTALL_DIR}/assay" ]]; then
      continue
    fi
    link_dir="$(dirname "$link_path")"
    if [[ -d "$link_dir" ]]; then
      ln -sfn "${INSTALL_DIR}/assay" "$link_path"
    fi
  done
}

version="${tag#v}"
asset="assay-${tag}-${TARGET}.tar.gz"
base_url="https://github.com/${REPO}/releases/download/${tag}"

current_version=""
if [[ -x "${INSTALL_DIR}/assay" ]]; then
  current_version="$("${INSTALL_DIR}/assay" --version 2>/dev/null | awk '{print $2}' || true)"
elif command -v assay >/dev/null 2>&1; then
  current_version="$(assay --version 2>/dev/null | awk '{print $2}' || true)"
fi

if [[ "${ASSAY_FORCE_UPDATE:-0}" != "1" && "$current_version" == "$version" ]]; then
  ensure_compat_symlinks
  echo "assay is already current: ${version}"
  exit 0
fi

tmpdir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmpdir"
}
trap cleanup EXIT
cd "$tmpdir"

curl -fsSLO "${base_url}/${asset}"
curl -fsSLO "${base_url}/${asset}.sha256"
sha256sum -c "${asset}.sha256"

tar -xzf "$asset"
extracted_dir="${asset%.tar.gz}"
if [[ ! -x "${extracted_dir}/assay" ]]; then
  echo "release archive did not contain executable assay binary" >&2
  exit 1
fi

install -d -m 0755 "$INSTALL_DIR"
install -m 0755 "${extracted_dir}/assay" "${INSTALL_DIR}/assay.new"
mv "${INSTALL_DIR}/assay.new" "${INSTALL_DIR}/assay"
chown "${INSTALL_OWNER}" "${INSTALL_DIR}/assay"

ensure_compat_symlinks

echo "updated assay to ${version} at ${INSTALL_DIR}/assay"
