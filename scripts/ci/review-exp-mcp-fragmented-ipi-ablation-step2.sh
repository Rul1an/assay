#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOW_PREFIXES=(
  "scripts/ci/fixtures/exp-mcp-fragmented-ipi/"
  "scripts/ci/exp-mcp-fragmented-ipi/"
  "scripts/ci/test-exp-mcp-fragmented-ipi-ablation.sh"
  "scripts/ci/review-exp-mcp-fragmented-ipi-ablation-step2.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: ablation Step2 must not touch workflows ($f)"
    exit 1
  fi
  ok="false"
  for p in "${ALLOW_PREFIXES[@]}"; do
    if [[ "$p" == */ ]]; then
      [[ "$f" == "$p"* ]] && ok="true"
    else
      [[ "$f" == "$p" ]] && ok="true"
    fi
    [[ "$ok" == "true" ]] && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in ablation Step2: $f"
    exit 1
  fi
done

echo "[review] required policy fixtures"
test -f scripts/ci/fixtures/exp-mcp-fragmented-ipi/policies/ablation_wrap_only.yaml || { echo "FAIL: missing wrap_only policy"; exit 1; }
test -f scripts/ci/fixtures/exp-mcp-fragmented-ipi/policies/ablation_sequence_only.yaml || { echo "FAIL: missing sequence_only policy"; exit 1; }
test -f scripts/ci/fixtures/exp-mcp-fragmented-ipi/policies/ablation_combined.yaml || { echo "FAIL: missing combined policy"; exit 1; }
rg -n 'SEQUENCE_SIDECAR=0' scripts/ci/exp-mcp-fragmented-ipi/ablation/run_variant.sh >/dev/null || { echo "FAIL: wrap_only sidecar toggle missing"; exit 1; }
rg -n 'SEQUENCE_SIDECAR=1' scripts/ci/exp-mcp-fragmented-ipi/ablation/run_variant.sh >/dev/null || { echo "FAIL: sidecar enable toggle missing"; exit 1; }
rg -n 'protected_sequence_sidecar_enabled|ablation_mode' scripts/ci/exp-mcp-fragmented-ipi/score_runs.py >/dev/null || { echo "FAIL: scorer missing ablation metadata"; exit 1; }

echo "[review] run CI-safe ablation runner (offline by default)"
bash scripts/ci/test-exp-mcp-fragmented-ipi-ablation.sh

test -f target/exp-mcp-fragmented-ipi-ablation/test/ablation-summary.json || { echo "FAIL: missing ablation summary"; exit 1; }

echo "[review] done"
