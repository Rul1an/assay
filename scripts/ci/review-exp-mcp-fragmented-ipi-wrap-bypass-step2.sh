#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOW_PREFIXES=(
  "scripts/ci/fixtures/exp-mcp-fragmented-ipi/wrap_bypass/"
  "scripts/ci/fixtures/exp-mcp-fragmented-ipi/policies/ablation_wrap_only.yaml"
  "scripts/ci/fixtures/exp-mcp-fragmented-ipi/policies/ablation_sequence_only.yaml"
  "scripts/ci/fixtures/exp-mcp-fragmented-ipi/policies/ablation_combined.yaml"
  "scripts/ci/exp-mcp-fragmented-ipi/drive_fragmented_ipi.py"
  "scripts/ci/exp-mcp-fragmented-ipi/score_wrap_bypass.py"
  "scripts/ci/test-exp-mcp-fragmented-ipi-wrap-bypass.sh"
  "scripts/ci/review-exp-mcp-fragmented-ipi-wrap-bypass-step2.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  [[ "$f" == .github/workflows/* ]] && { echo "FAIL: no workflows"; exit 1; }

  ok="false"
  for p in "${ALLOW_PREFIXES[@]}"; do
    if [[ "$p" == */ ]]; then
      [[ "$f" == "$p"* ]] && ok="true"
    else
      [[ "$f" == "$p" ]] && ok="true"
    fi
    [[ "$ok" == "true" ]] && break
  done

  [[ "$ok" != "true" ]] && { echo "FAIL: file not allowed: $f"; exit 1; }
done

echo "[review] required files"
test -f scripts/ci/fixtures/exp-mcp-fragmented-ipi/wrap_bypass/expected_fragments.json || { echo "FAIL: missing expected fragments fixture"; exit 1; }
test -f scripts/ci/exp-mcp-fragmented-ipi/score_wrap_bypass.py || { echo "FAIL: missing score_wrap_bypass.py"; exit 1; }

echo "[review] marker checks"
rg -n 'FRAG_A=|FRAG_B=' scripts/ci/fixtures/exp-mcp-fragmented-ipi/wrap_bypass/attack_doc_part1.txt scripts/ci/fixtures/exp-mcp-fragmented-ipi/wrap_bypass/attack_doc_part2.txt >/dev/null || { echo "FAIL: fixtures missing fragment markers"; exit 1; }
rg -n 'sink_calls|experiment_variant|wrap_bypass' scripts/ci/exp-mcp-fragmented-ipi/drive_fragmented_ipi.py >/dev/null || { echo "FAIL: driver missing wrap-bypass markers"; exit 1; }
rg -n 'reconstruction_success|blocked_by_wrap|blocked_by_sequence' scripts/ci/exp-mcp-fragmented-ipi/score_wrap_bypass.py >/dev/null || { echo "FAIL: scorer missing reconstruction or attribution markers"; exit 1; }

echo "[review] run offline wrap-bypass test"
RUN_LIVE=0 bash scripts/ci/test-exp-mcp-fragmented-ipi-wrap-bypass.sh

test -f target/exp-mcp-fragmented-ipi-wrap-bypass/test/wrap-bypass-summary.json || { echo "FAIL: missing wrap-bypass summary"; exit 1; }

echo "[review] done"
