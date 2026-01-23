#!/usr/bin/env bash
set -euo pipefail

OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
if [[ "$OS" != "linux" ]]; then
  echo "cargo-clippy: skipped on $OS (Linux-only dependency stack: aya/eBPF)"
  exit 0
fi

if [[ "${PRECOMMIT_CLIPPY:-0}" != "1" ]]; then
  echo "cargo-clippy: skipped (set PRECOMMIT_CLIPPY=1 to enable)"
  exit 0
fi

# strict, but only on Linux
cargo clippy --locked --workspace --all-targets -- -D warnings
