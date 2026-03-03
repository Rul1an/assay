#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOW_PREFIXES=(
  "scripts/ci/exp-mcp-fragmented-ipi/compat_host/compat_host.py"
  "scripts/ci/exp-mcp-fragmented-ipi/drive_fragmented_ipi.py"
  "scripts/ci/exp-mcp-fragmented-ipi/score_sink_failure.py"
  "scripts/ci/test-exp-mcp-fragmented-ipi-sink-failure.sh"
  "scripts/ci/review-exp-mcp-fragmented-ipi-sink-failure-step2.sh"
  "scripts/ci/fixtures/exp-mcp-fragmented-ipi/sink_failure/"
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

echo "[review] marker checks"
rg -n 'SINK_PRIMARY_OUTCOME|SINK_ALT_OUTCOME' scripts/ci/exp-mcp-fragmented-ipi/compat_host/compat_host.py >/dev/null || {
  echo "FAIL: compat-host missing sink outcome env wiring"
  exit 1
}
rg -n 'sink_outcome=' scripts/ci/exp-mcp-fragmented-ipi/compat_host/compat_host.py >/dev/null || {
  echo "FAIL: compat-host missing sink_outcome audit log"
  exit 1
}
rg -n 'sink_failure|SINK_PRIMARY_OUTCOME|SINK_ALT_OUTCOME' scripts/ci/exp-mcp-fragmented-ipi/drive_fragmented_ipi.py >/dev/null || {
  echo "FAIL: driver missing sink-failure wiring"
  exit 1
}
test -f scripts/ci/exp-mcp-fragmented-ipi/score_sink_failure.py || { echo "FAIL: scorer missing"; exit 1; }

echo "[review] run offline sink-failure test"
RUN_LIVE=0 bash scripts/ci/test-exp-mcp-fragmented-ipi-sink-failure.sh

test -f "$ROOT/target/exp-mcp-fragmented-ipi-sink-failure/test/sink-failure-summary.json" || {
  echo "FAIL: expected sink-failure-summary.json"
  exit 1
}

echo "[review] done"
