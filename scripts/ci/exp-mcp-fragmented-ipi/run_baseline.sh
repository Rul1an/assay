#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
OUT_DIR="${1:-$ROOT/target/exp-mcp-fragmented-ipi/baseline}"
RUNS_ATTACK="${RUNS_ATTACK:-2}"
RUNS_LEGIT="${RUNS_LEGIT:-1}"
RUN_SET="${RUN_SET:-deterministic}"
RUN_LIVE="${RUN_LIVE:-0}"
ABLATION_MODE="${ABLATION_MODE:-unknown}"
MCP_HOST_CMD="${MCP_HOST_CMD:-}"
MCP_HOST_ARGS="${MCP_HOST_ARGS:-}"
ASSAY_CMD="${ASSAY_CMD:-assay}"
FIXTURE_ROOT="$ROOT/scripts/ci/fixtures/exp-mcp-fragmented-ipi"
POLICY="$FIXTURE_ROOT/policies/baseline_wrap.yaml"
mkdir -p "$OUT_DIR"

test -x "$ROOT/target/debug/assay" || { echo "Missing $ROOT/target/debug/assay; build assay-cli first"; exit 1; }
test -x "$ROOT/target/debug/assay-mcp-server" || { echo "Missing $ROOT/target/debug/assay-mcp-server; build assay-mcp-server first"; exit 1; }

echo "ABLATION_MODE=$ABLATION_MODE"
echo "RUN_LIVE=$RUN_LIVE"
if [[ "$RUN_LIVE" == "1" ]]; then
  : "${MCP_HOST_CMD:?MCP_HOST_CMD is required for RUN_LIVE=1}"
  echo "MCP_HOST_CMD=$MCP_HOST_CMD"
  echo "MCP_HOST_ARGS=$MCP_HOST_ARGS"
fi

python3 "$ROOT/scripts/ci/exp-mcp-fragmented-ipi/drive_fragmented_ipi.py" \
  --repo-root "$ROOT" \
  --fixture-root "$FIXTURE_ROOT" \
  --wrap-policy "$POLICY" \
  --run-live "$RUN_LIVE" \
  --mcp-host-cmd "$MCP_HOST_CMD" \
  --mcp-host-args "$MCP_HOST_ARGS" \
  --assay-cmd "$ASSAY_CMD" \
  --output-dir "$OUT_DIR" \
  --output-jsonl "$OUT_DIR/baseline_attack.jsonl" \
  --mode baseline \
  --scenario attack \
  --run-set "$RUN_SET" \
  --runs "$RUNS_ATTACK"

python3 "$ROOT/scripts/ci/exp-mcp-fragmented-ipi/drive_fragmented_ipi.py" \
  --repo-root "$ROOT" \
  --fixture-root "$FIXTURE_ROOT" \
  --wrap-policy "$POLICY" \
  --run-live "$RUN_LIVE" \
  --mcp-host-cmd "$MCP_HOST_CMD" \
  --mcp-host-args "$MCP_HOST_ARGS" \
  --assay-cmd "$ASSAY_CMD" \
  --output-dir "$OUT_DIR" \
  --output-jsonl "$OUT_DIR/baseline_legit.jsonl" \
  --mode baseline \
  --scenario legit \
  --run-set "$RUN_SET" \
  --runs "$RUNS_LEGIT"

python3 "$ROOT/scripts/ci/exp-mcp-fragmented-ipi/score_runs.py" \
  "$OUT_DIR/baseline_attack.jsonl" \
  "$OUT_DIR/baseline_legit.jsonl" > "$OUT_DIR/summary.json"
