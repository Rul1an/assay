#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOW_PREFIXES=(
  "scripts/ci/exp-mcp-fragmented-ipi/compat_host/compat_host.py"
  "scripts/ci/exp-mcp-fragmented-ipi/mock_mcp_server.py"
  "scripts/ci/exp-mcp-fragmented-ipi/drive_fragmented_ipi.py"
  "scripts/ci/exp-mcp-fragmented-ipi/ablation/run_variant.sh"
  "scripts/ci/exp-mcp-fragmented-ipi/score_second_sink.py"
  "scripts/ci/fixtures/exp-mcp-fragmented-ipi/policies/second_sink_sequence.yaml"
  "scripts/ci/test-exp-mcp-fragmented-ipi-compat-host.sh"
  "scripts/ci/test-exp-mcp-fragmented-ipi-second-sink.sh"
  "scripts/ci/review-exp-mcp-fragmented-ipi-second-sink-step2.sh"
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
rg -n 'web_search_alt' scripts/ci/exp-mcp-fragmented-ipi/compat_host/compat_host.py >/dev/null || {
  echo "FAIL: compat_host missing web_search_alt"
  exit 1
}
rg -n 'web_search_alt' scripts/ci/exp-mcp-fragmented-ipi/mock_mcp_server.py >/dev/null || {
  echo "FAIL: mock_mcp_server missing web_search_alt"
  exit 1
}
rg -n 'second_sink|SECOND_SINK_PATH' scripts/ci/exp-mcp-fragmented-ipi/drive_fragmented_ipi.py >/dev/null || {
  echo "FAIL: driver missing second_sink markers"
  exit 1
}
rg -n 'sink_path_class|success_any_sink_canary' scripts/ci/exp-mcp-fragmented-ipi/score_second_sink.py >/dev/null || {
  echo "FAIL: scorer missing sink-path markers"
  exit 1
}
rg -n 'aliases:|web_search_alt|web_search' scripts/ci/fixtures/exp-mcp-fragmented-ipi/policies/second_sink_sequence.yaml >/dev/null || {
  echo "FAIL: second sink sequence policy missing alias coverage"
  exit 1
}

echo "[review] compat-host smoke"
bash scripts/ci/test-exp-mcp-fragmented-ipi-compat-host.sh

echo "[review] second-sink offline run"
RUN_LIVE=0 bash scripts/ci/test-exp-mcp-fragmented-ipi-second-sink.sh

test -f "$ROOT/target/exp-mcp-fragmented-ipi-second-sink/test/second-sink-summary.json" || {
  echo "FAIL: expected second-sink-summary.json"
  exit 1
}

echo "[review] done"
