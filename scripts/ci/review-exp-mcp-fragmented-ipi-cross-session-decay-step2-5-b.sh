#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOW_PREFIXES=(
  "scripts/ci/exp-mcp-fragmented-ipi/cross_session/run_cross_session_decay.sh"
  "scripts/ci/exp-mcp-fragmented-ipi/score_cross_session_decay.py"
  "scripts/ci/test-exp-mcp-fragmented-ipi-cross-session-decay-kplus.sh"
  "scripts/ci/review-exp-mcp-fragmented-ipi-cross-session-decay-step2-5-b.sh"
)

while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  [[ "$f" == .github/workflows/* ]] && { echo "FAIL: no workflows"; exit 1; }
  ok="false"
  for p in "${ALLOW_PREFIXES[@]}"; do
    [[ "$f" == "$p" ]] && ok="true" && break
  done
  [[ "$ok" != "true" ]] && { echo "FAIL: file not allowed: $f"; exit 1; }
done < <(git diff --name-only "$BASE_REF"...HEAD)

rg -n 'session_sink_k2' scripts/ci/exp-mcp-fragmented-ipi/cross_session/run_cross_session_decay.sh >/dev/null || {
  echo "FAIL: runner missing k+2 delayed sink session"
  exit 1
}
rg -n 'session_sink_k3' scripts/ci/exp-mcp-fragmented-ipi/cross_session/run_cross_session_decay.sh >/dev/null || {
  echo "FAIL: runner missing k+3 delayed sink session"
  exit 1
}
rg -n 'success_k_plus_2|success_k_plus_3' scripts/ci/exp-mcp-fragmented-ipi/score_cross_session_decay.py >/dev/null || {
  echo "FAIL: scorer missing k+2/k+3 reporting"
  exit 1
}

SKIP_CARGO_BUILD="${SKIP_CARGO_BUILD:-0}" RUN_LIVE=0 \
  bash scripts/ci/test-exp-mcp-fragmented-ipi-cross-session-decay-kplus.sh
echo "[review] done"
