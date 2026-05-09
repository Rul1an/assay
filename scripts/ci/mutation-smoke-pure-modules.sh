#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

if [ "${ASSAY_RUN_MUTATION_SMOKE:-0}" != "1" ]; then
  echo "[mutation-smoke] skipped: set ASSAY_RUN_MUTATION_SMOKE=1 to run targeted mutation smoke"
  exit 0
fi

if ! cargo mutants --version >/dev/null 2>&1; then
  echo "[mutation-smoke] skipped: cargo-mutants is not installed"
  exit 0
fi

COMMON=(--timeout 60 --minimum-test-timeout 20 --jobs "${ASSAY_MUTATION_JOBS:-2}")

echo "[mutation-smoke] trust_basis diff.rs"
cargo mutants --package assay-evidence --file crates/assay-evidence/src/trust_basis/diff.rs "${COMMON[@]}" -- trust_basis

echo "[mutation-smoke] trust_basis classifiers.rs"
cargo mutants --package assay-evidence --file crates/assay-evidence/src/trust_basis/classifiers.rs "${COMMON[@]}" -- trust_basis

echo "[mutation-smoke] sandbox degradation.rs"
cargo mutants --package assay-cli --file crates/assay-cli/src/cli/commands/sandbox/degradation.rs "${COMMON[@]}" -- sandbox
