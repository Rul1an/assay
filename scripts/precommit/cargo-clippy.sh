#!/usr/bin/env bash
set -euo pipefail

# This script runs cargo clippy on the workspace.
# It is designed to be fast enough for pre-push hooks by relying on incrementals.

echo "cargo-clippy: checking workspace..."

# Run clippy with -D warnings to catch lints that CI would fail on.
# We include --all-targets to catch lints in tests and examples.
cargo clippy --workspace --all-targets -- -D warnings
