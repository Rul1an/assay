#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOW_PREFIXES=(
  "scripts/ci/exp-mcp-fragmented-ipi/"
  "scripts/ci/test-exp-mcp-fragmented-ipi-ablation.sh"
  "scripts/ci/review-exp-mcp-fragmented-ipi-ablation-live-b.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: ablation live-enable StepB must not touch workflows ($f)"
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
    echo "FAIL: file not allowed in ablation live-enable StepB: $f"
    exit 1
  fi
done

echo "[review] required scripts exist"
test -f scripts/ci/test-exp-mcp-fragmented-ipi-ablation.sh
test -f scripts/ci/exp-mcp-fragmented-ipi/ablation/run_variant.sh
test -f scripts/ci/exp-mcp-fragmented-ipi/run_protected.sh
test -f scripts/ci/exp-mcp-fragmented-ipi/run_baseline.sh

echo "[review] RUN_LIVE contract check (non-fragile)"
rg -n 'MCP_HOST_CMD is required for RUN_LIVE=1' \
  scripts/ci/test-exp-mcp-fragmented-ipi-ablation.sh \
  scripts/ci/exp-mcp-fragmented-ipi/ablation/run_variant.sh \
  scripts/ci/exp-mcp-fragmented-ipi/run_baseline.sh \
  scripts/ci/exp-mcp-fragmented-ipi/run_protected.sh >/dev/null || {
  echo "FAIL: expected live preflight requirement for MCP_HOST_CMD missing"
  exit 1
}

echo "[review] audit markers enforced"
rg -n '^echo "ABLATION_MODE=' scripts/ci/exp-mcp-fragmented-ipi/run_protected.sh scripts/ci/exp-mcp-fragmented-ipi/run_baseline.sh >/dev/null || {
  echo "FAIL: ABLATION_MODE marker not written in run scripts"
  exit 1
}
rg -n 'SIDECAR=enabled|SIDECAR=disabled' scripts/ci/exp-mcp-fragmented-ipi/run_protected.sh >/dev/null || {
  echo "FAIL: SIDECAR enabled/disabled markers missing in run_protected.sh"
  exit 1
}

echo "[review] CI-safe offline run (default RUN_LIVE=0)"
RUN_LIVE=0 bash scripts/ci/test-exp-mcp-fragmented-ipi-ablation.sh

test -f target/exp-mcp-fragmented-ipi-ablation/test/ablation-summary.json || {
  echo "FAIL: expected ablation-summary.json after offline run"
  exit 1
}

echo "[review] done"
