#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOW_PREFIXES=(
  "scripts/ci/exp-mcp-fragmented-ipi/cross_session/state.py"
  "scripts/ci/exp-mcp-fragmented-ipi/cross_session/run_cross_session_decay.sh"
  "scripts/ci/exp-mcp-fragmented-ipi/score_cross_session_decay.py"
  "scripts/ci/test-exp-mcp-fragmented-ipi-cross-session-decay.sh"
  "scripts/ci/review-exp-mcp-fragmented-ipi-cross-session-decay-step2.sh"
  "scripts/ci/fixtures/exp-mcp-fragmented-ipi/cross_session/"
  "scripts/ci/exp-mcp-fragmented-ipi/drive_fragmented_ipi.py"
)

while IFS= read -r f; do
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
done < <(git diff --name-only "$BASE_REF"...HEAD)

rg -n 'same_session_control' scripts/ci/exp-mcp-fragmented-ipi/cross_session/run_cross_session_decay.sh >/dev/null || { echo 'FAIL: missing same-session control in cross-session runner'; exit 1; }

RUN_LIVE=0 bash scripts/ci/test-exp-mcp-fragmented-ipi-cross-session-decay.sh
echo "[review] done"
