#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

RUNNER="crates/assay-core/src/engine/runner.rs"
SINGLE="crates/assay-core/src/engine/runner_next/single.rs"
ASSERTIONS="crates/assay-core/src/engine/runner_next/assertions.rs"

echo "[review] workflow and generated-file guard"
if ! git diff --quiet -- .github/workflows; then
  echo "FAIL: Wave 51 Runner Step1 must not touch workflows"
  exit 1
fi
if ! git diff --quiet -- crates/assay-ebpf/src/vmlinux.rs; then
  echo "FAIL: generated vmlinux.rs must stay out of scope"
  exit 1
fi

echo "[review] facade thinness"
runner_code_lines="$(
  awk 'BEGIN{n=0; in_tests=0} /^#\[cfg\(test\)\]/{in_tests=1} !in_tests{n++} END{print n}' "$RUNNER"
)"
echo "runner non-test lines: $runner_code_lines"
if [ "$runner_code_lines" -gt 140 ]; then
  echo "FAIL: runner facade is too thick"
  exit 1
fi

echo "[review] boundary markers"
rg -n 'runner_next::assertions::apply_agent_assertions_impl' "$RUNNER" >/dev/null || {
  echo "FAIL: runner assertion facade does not delegate to runner_next"
  exit 1
}
rg -n 'runner_next::single::run_test_once_impl' "$RUNNER" >/dev/null || {
  echo "FAIL: runner single-test facade does not delegate to runner_next"
  exit 1
}
rg -n 'cache_key|assay.eval.metric|check_baseline_regressions' "$SINGLE" >/dev/null || {
  echo "FAIL: single-test implementation markers missing"
  exit 1
}
rg -n 'verify_assertions|assertions failed|assertions error' "$ASSERTIONS" >/dev/null || {
  echo "FAIL: assertion overlay markers missing"
  exit 1
}

echo "[review] repo checks"
cargo fmt --check
cargo check -p assay-core
cargo test -p assay-core --lib runner_contract_
git diff --check

echo "[review] PASS"
