#!/usr/bin/env bash
set -euo pipefail

# Phase 5 Quality Check Script
# Ensures strict adherence to quality before proceeding to next PRs.
# Includes: Check, Test (Landlock Logic), Fmt, Clippy.

# Runner-proof: Use local target dir to avoid FS issues on VM mounts
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/tmp/assay-target}"
export CARGO_INCREMENTAL=0

echo ">>> 🛡️  Phase 5 SOTA Hardening: Quality Gate"
echo "    Target: $CARGO_TARGET_DIR"

echo ">>> [1/4] Cargo Check"
cargo check --bin assay --locked

echo ">>> [2/4] Verifying Landlock Logic (Semantics Correctness)"
cargo test --bin assay landlock_check --locked

echo ">>> [3/4] Formatting Check"
cargo fmt -- --check

echo ">>> [4/4] Clippy (Strict)"
# Deny warnings to prevent "needless return" and unused vars re-occurring
cargo clippy --workspace --locked -- -D warnings

echo ">>> ✅ Phase 5 Check Passed!"
