#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/codex/exp-mcp-fragmented-ipi-step2-harness}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/contributing/SPLIT-CHECKLIST-exp-mcp-fragmented-ipi-step3.md"
  "docs/contributing/SPLIT-REVIEW-PACK-exp-mcp-fragmented-ipi-step3.md"
  "scripts/ci/review-exp-mcp-fragmented-ipi-step3.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Experiment Step3 must not change workflows ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in experiment Step3: $f"
    exit 1
  fi
done

echo "[review] invariants"
test -f docs/architecture/PLAN-EXPERIMENT-MCP-FRAGMENTED-IPI-2026q1.md || { echo "FAIL: missing Step1 plan"; exit 1; }
test -f docs/architecture/EXPERIMENT-MCP-FRAGMENTED-IPI-POLICY-CONTRACT.md || { echo "FAIL: missing Step1 policy contract"; exit 1; }
test -f scripts/ci/review-exp-mcp-fragmented-ipi-step1.sh || { echo "FAIL: missing Step1 reviewer gate"; exit 1; }
test -f scripts/ci/fixtures/exp-mcp-fragmented-ipi/invoice_with_canary.txt || { echo "FAIL: missing invoice canary fixture"; exit 1; }
test -f scripts/ci/fixtures/exp-mcp-fragmented-ipi/policies/fragmented_sequence.yaml || { echo "FAIL: missing sequence policy fixture"; exit 1; }
test -f scripts/ci/exp-mcp-fragmented-ipi/drive_fragmented_ipi.py || { echo "FAIL: missing Step2 driver"; exit 1; }
test -f scripts/ci/exp-mcp-fragmented-ipi/score_runs.py || { echo "FAIL: missing Step2 scorer"; exit 1; }
test -f scripts/ci/test-exp-mcp-fragmented-ipi.sh || { echo "FAIL: missing Step2 test runner"; exit 1; }
rg -n 'assay_check_sequence|blocked_by_sequence' scripts/ci/exp-mcp-fragmented-ipi/drive_fragmented_ipi.py >/dev/null || { echo "FAIL: driver missing sequence-sidecar markers"; exit 1; }
rg -n 'baseline_asr|protected_tpr|protected_false_positive_rate' scripts/ci/exp-mcp-fragmented-ipi/score_runs.py >/dev/null || { echo "FAIL: scorer missing security metrics"; exit 1; }

bash scripts/ci/test-exp-mcp-fragmented-ipi.sh >/dev/null

echo "[review] done"
