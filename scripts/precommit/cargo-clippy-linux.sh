#!/usr/bin/env bash
set -euo pipefail

OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
if [[ "$OS" != "linux" ]]; then
  echo "cargo-clippy: skipped on $OS (Linux-only dependency stack: aya/eBPF)"
  exit 0
fi

# strict, but only on Linux
cargo clippy --workspace --all-targets -- -D warnings
