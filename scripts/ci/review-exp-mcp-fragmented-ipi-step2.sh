#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "scripts/ci/fixtures/exp-mcp-fragmented-ipi/canary.txt"
  "scripts/ci/fixtures/exp-mcp-fragmented-ipi/invoice_with_canary.txt"
  "scripts/ci/fixtures/exp-mcp-fragmented-ipi/doc_a.txt"
  "scripts/ci/fixtures/exp-mcp-fragmented-ipi/doc_b.txt"
  "scripts/ci/fixtures/exp-mcp-fragmented-ipi/public_article.txt"
  "scripts/ci/fixtures/exp-mcp-fragmented-ipi/policies/baseline_wrap.yaml"
  "scripts/ci/fixtures/exp-mcp-fragmented-ipi/policies/protected_wrap.yaml"
  "scripts/ci/fixtures/exp-mcp-fragmented-ipi/policies/fragmented_sequence.yaml"
  "scripts/ci/exp-mcp-fragmented-ipi/mock_mcp_server.py"
  "scripts/ci/exp-mcp-fragmented-ipi/drive_fragmented_ipi.py"
  "scripts/ci/exp-mcp-fragmented-ipi/run_baseline.sh"
  "scripts/ci/exp-mcp-fragmented-ipi/run_protected.sh"
  "scripts/ci/exp-mcp-fragmented-ipi/score_runs.py"
  "scripts/ci/test-exp-mcp-fragmented-ipi.sh"
  "scripts/ci/review-exp-mcp-fragmented-ipi-step2.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Step2 must not change workflows ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in experiment Step2: $f"
    exit 1
  fi
done

echo "[review] required markers"
rg -n "CANARY_" scripts/ci/fixtures/exp-mcp-fragmented-ipi/invoice_with_canary.txt scripts/ci/fixtures/exp-mcp-fragmented-ipi/canary.txt >/dev/null || {
  echo "FAIL: canary fixtures missing"
  exit 1
}
rg -n "never_after|read_sensitive_document|web_search" scripts/ci/fixtures/exp-mcp-fragmented-ipi/policies/fragmented_sequence.yaml >/dev/null || {
  echo "FAIL: sequence policy missing never_after rule"
  exit 1
}
rg -n "blocked_by_sequence|blocked_by_wrap|attack_success" scripts/ci/exp-mcp-fragmented-ipi/drive_fragmented_ipi.py >/dev/null || {
  echo "FAIL: driver missing experiment result markers"
  exit 1
}
rg -n "protected_tpr|protected_false_positive_rate|baseline_asr" scripts/ci/exp-mcp-fragmented-ipi/score_runs.py >/dev/null || {
  echo "FAIL: scorer missing security metrics"
  exit 1
}

bash scripts/ci/test-exp-mcp-fragmented-ipi.sh

echo "[review] done"
