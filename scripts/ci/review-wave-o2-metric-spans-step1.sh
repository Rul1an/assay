#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/codex/codebase-analysis-observability}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "crates/assay-core/src/engine/runner.rs"
  "crates/assay-core/src/engine/runner_next/execute.rs"
  "crates/assay-core/tests/runner_metric_spans.rs"
  "docs/contributing/SPLIT-INVENTORY-wave-o2-metric-spans-step1.md"
  "docs/contributing/SPLIT-CHECKLIST-wave-o2-metric-spans-step1.md"
  "docs/contributing/SPLIT-MOVE-MAP-wave-o2-metric-spans-step1.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave-o2-metric-spans-step1.md"
  "scripts/ci/review-wave-o2-metric-spans-step1.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Wave O2 Step1 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave O2 Step1: $f"
    exit 1
  fi
done < <({
  git diff --name-only "$BASE_REF"...HEAD
  git diff --name-only
  git diff --name-only --cached
  git ls-files --others --exclude-standard
} | awk 'NF' | sort -u)

echo "[review] marker checks"
rg -n 'assay.eval.metric' crates/assay-core/src/engine/runner.rs >/dev/null || {
  echo "FAIL: missing assay.eval.metric span"
  exit 1
}
rg -n 'with_current_subscriber' crates/assay-core/src/engine/runner_next/execute.rs >/dev/null || {
  echo "FAIL: missing subscriber propagation in run_suite"
  exit 1
}
rg -n 'runner_metric_spans_record_success_fields|runner_metric_spans_record_error_fields' \
  crates/assay-core/tests/runner_metric_spans.rs >/dev/null || {
  echo "FAIL: missing metric span contract tests"
  exit 1
}
rg -n 'error.message|assay.eval.metric.duration_ms' \
  crates/assay-core/src/engine/runner.rs \
  crates/assay-core/tests/runner_metric_spans.rs >/dev/null || {
  echo "FAIL: missing metric span field recording"
  exit 1
}

echo "[review] repo checks"
cargo fmt --check
cargo clippy -q -p assay-core --all-targets -- -D warnings
cargo check -q -p assay-core
cargo test -q -p assay-core --lib
cargo test -q -p assay-core --test runner_metric_spans
cargo test -q -p assay-core --test otel_contract
git diff --check

echo "[review] PASS"
