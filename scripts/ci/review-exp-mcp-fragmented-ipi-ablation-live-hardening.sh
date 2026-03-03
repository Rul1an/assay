#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "scripts/ci/test-exp-mcp-fragmented-ipi-ablation.sh"
  "scripts/ci/exp-mcp-fragmented-ipi/ablation/run_variant.sh"
  "scripts/ci/exp-mcp-fragmented-ipi/run_baseline.sh"
  "scripts/ci/exp-mcp-fragmented-ipi/run_protected.sh"
  "scripts/ci/exp-mcp-fragmented-ipi/drive_fragmented_ipi.py"
  "scripts/ci/review-exp-mcp-fragmented-ipi-ablation-live-hardening.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: live hardening slice must not touch workflows ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in live hardening slice: $f"
    exit 1
  fi
done

echo "[review] RUN_LIVE validation markers"
rg -n 'FAIL: RUN_LIVE must be 0 or 1' \
  scripts/ci/test-exp-mcp-fragmented-ipi-ablation.sh \
  scripts/ci/exp-mcp-fragmented-ipi/ablation/run_variant.sh \
  scripts/ci/exp-mcp-fragmented-ipi/run_baseline.sh \
  scripts/ci/exp-mcp-fragmented-ipi/run_protected.sh >/dev/null || {
  echo "FAIL: RUN_LIVE validation missing"
  exit 1
}

echo "[review] no MCP_HOST_ARGS logging"
if rg -n 'echo "MCP_HOST_ARGS=' \
  scripts/ci/exp-mcp-fragmented-ipi/ablation/run_variant.sh \
  scripts/ci/exp-mcp-fragmented-ipi/run_baseline.sh \
  scripts/ci/exp-mcp-fragmented-ipi/run_protected.sh >/dev/null; then
  echo "FAIL: MCP_HOST_ARGS must not be logged"
  exit 1
fi

echo "[review] ASSAY_CMD parsing"
rg -n 'shlex\.split\(assay_cmd\)' scripts/ci/exp-mcp-fragmented-ipi/drive_fragmented_ipi.py >/dev/null || {
  echo "FAIL: ASSAY_CMD is not split into argv"
  exit 1
}

echo "[review] offline runner remains green"
RUN_LIVE=0 bash scripts/ci/test-exp-mcp-fragmented-ipi-ablation.sh

test -f target/exp-mcp-fragmented-ipi-ablation/test/ablation-summary.json || {
  echo "FAIL: missing ablation summary after offline run"
  exit 1
}

echo "[review] done"
