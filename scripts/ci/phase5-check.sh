#!/bin/bash
set -euo pipefail

# Phase 5 Quality Check Script
# Ensures strict adherence to quality before proceeding to next PRs.
# Includes: Check, Test (Landlock Logic), Fmt, Clippy.

echo ">>> 🛡️  Phase 5 SOTA Hardening: Quality Gate"

echo ">>> [1/4] Cargo Check"
cargo check --bin assay

echo ">>> [2/4] Verifying Landlock Logic (Semantics Correctness)"
cargo test --bin assay landlock_check

echo ">>> [3/4] Formatting Check"
cargo fmt -- --check

echo ">>> [4/4] Clippy (Strict)"
# Deny warnings to prevent "needless return" and unused vars re-occurring
cargo clippy --workspace -- -D warnings

echo ">>> ✅ Phase 5 Check Passed!"
