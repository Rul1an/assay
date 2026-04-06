#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  build_mcpb_bundle.sh \
    --version-tag vX.Y.Z \
    --linux-x86-archive <path> \
    --linux-arm64-archive <path> \
    --output <path/to/output.mcpb>

Build a real MCPB bundle for assay-mcp-server from release archives.
EOF
}

VERSION_TAG=""
LINUX_X86_ARCHIVE=""
LINUX_ARM64_ARCHIVE=""
OUTPUT=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version-tag)
      VERSION_TAG="${2:-}"
      shift 2
      ;;
    --linux-x86-archive)
      LINUX_X86_ARCHIVE="${2:-}"
      shift 2
      ;;
    --linux-arm64-archive)
      LINUX_ARM64_ARCHIVE="${2:-}"
      shift 2
      ;;
    --output)
      OUTPUT="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "$VERSION_TAG" || -z "$LINUX_X86_ARCHIVE" || -z "$LINUX_ARM64_ARCHIVE" || -z "$OUTPUT" ]]; then
  usage >&2
  exit 2
fi

if [[ ! -f "$LINUX_X86_ARCHIVE" || ! -f "$LINUX_ARM64_ARCHIVE" ]]; then
  echo "input release archive missing" >&2
  exit 1
fi

if ! command -v npx >/dev/null 2>&1; then
  echo "npx is required to build the MCPB bundle" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
SEMVER="${VERSION_TAG#v}"
MCPB_CLI_VERSION="${MCPB_CLI_VERSION:-2.1.2}"
MCPB_CLI_PACKAGE="@anthropic-ai/mcpb@${MCPB_CLI_VERSION}"
WORK_DIR="$(mktemp -d)"
STAGE_DIR="${WORK_DIR}/assay-mcp-server-mcpb"
X86_DIR="${WORK_DIR}/linux-x86_64"
ARM_DIR="${WORK_DIR}/linux-aarch64"

cleanup() {
  rm -rf "$WORK_DIR"
}
trap cleanup EXIT

mkdir -p \
  "$STAGE_DIR/server/linux-x86_64" \
  "$STAGE_DIR/server/linux-aarch64" \
  "$X86_DIR" \
  "$ARM_DIR"

tar -xzf "$LINUX_X86_ARCHIVE" -C "$X86_DIR"
tar -xzf "$LINUX_ARM64_ARCHIVE" -C "$ARM_DIR"

X86_BIN="$(find "$X86_DIR" -type f -name assay-mcp-server -print -quit)"
ARM_BIN="$(find "$ARM_DIR" -type f -name assay-mcp-server -print -quit)"

if [[ -z "$X86_BIN" || -z "$ARM_BIN" ]]; then
  echo "failed to locate assay-mcp-server binary inside release archives" >&2
  exit 1
fi

cp "$X86_BIN" "$STAGE_DIR/server/linux-x86_64/assay-mcp-server"
cp "$ARM_BIN" "$STAGE_DIR/server/linux-aarch64/assay-mcp-server"
cp "${REPO_ROOT}/packaging/mcpb/run-assay-mcp-server.sh" "$STAGE_DIR/server/run-assay-mcp-server.sh"

chmod 0755 \
  "$STAGE_DIR/server/linux-x86_64/assay-mcp-server" \
  "$STAGE_DIR/server/linux-aarch64/assay-mcp-server" \
  "$STAGE_DIR/server/run-assay-mcp-server.sh"

sed "s/__VERSION__/${SEMVER}/g" \
  "${REPO_ROOT}/packaging/mcpb/manifest.assay-mcp-server.template.json" \
  > "$STAGE_DIR/manifest.json"

npx --yes "${MCPB_CLI_PACKAGE}" validate "$STAGE_DIR/manifest.json"

mkdir -p "$(dirname "$OUTPUT")"
npx --yes "${MCPB_CLI_PACKAGE}" pack "$STAGE_DIR" "$OUTPUT"
shasum -a 256 "$OUTPUT" > "${OUTPUT}.sha256"
