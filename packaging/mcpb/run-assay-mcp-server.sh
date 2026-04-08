#!/usr/bin/env sh
set -eu

script_dir=$(
  CDPATH='' cd -- "$(dirname -- "$0")" && pwd
)

kernel=$(uname -s)
arch=$(uname -m)

if [ "$kernel" != "Linux" ]; then
  echo "assay-mcp-server MCPB only supports Linux in this package" >&2
  exit 78
fi

case "$arch" in
  x86_64|amd64)
    bin_path="$script_dir/linux-x86_64/assay-mcp-server"
    ;;
  aarch64|arm64)
    bin_path="$script_dir/linux-aarch64/assay-mcp-server"
    ;;
  *)
    echo "unsupported Linux architecture: $arch" >&2
    exit 78
    ;;
esac

exec "$bin_path" "$@"
