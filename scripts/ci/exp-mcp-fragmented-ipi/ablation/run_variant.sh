#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
ART_DIR="${1:-}"
FIX_DIR="${2:-}"
MODE="${3:-}"

if [[ -z "$ART_DIR" || -z "$FIX_DIR" || -z "$MODE" ]]; then
  echo "Usage: $0 <ARTIFACTS_DIR> <FIXTURES_DIR> <MODE>"
  exit 2
fi

case "$MODE" in
  wrap_only|sequence_only|combined) ;;
  *) echo "FAIL: unknown mode: $MODE"; exit 2 ;;
esac

MODE_DIR="$ART_DIR/$MODE"
mkdir -p "$MODE_DIR"

RUN_LIVE="${RUN_LIVE:-0}"
MCP_HOST_CMD="${MCP_HOST_CMD:-}"
MCP_HOST_ARGS="${MCP_HOST_ARGS:-}"
ASSAY_CMD="${ASSAY_CMD:-assay}"

case "$RUN_LIVE" in
  0) ;;
  1)
    : "${MCP_HOST_CMD:?MCP_HOST_CMD is required for RUN_LIVE=1}"
    ;;
  *)
    echo "FAIL: RUN_LIVE must be 0 or 1"
    exit 2
    ;;
esac

SEQUENCE_SIDECAR=0
ASSAY_POLICY=""
SEQUENCE_POLICY_FILE="fragmented_sequence.yaml"

case "$MODE" in
  wrap_only)
    ASSAY_POLICY="$FIX_DIR/policies/ablation_wrap_only.yaml"
    SEQUENCE_SIDECAR=0
    ;;
  sequence_only)
    ASSAY_POLICY="$FIX_DIR/policies/ablation_sequence_only.yaml"
    SEQUENCE_SIDECAR=1
    ;;
  combined)
    ASSAY_POLICY="$FIX_DIR/policies/ablation_combined.yaml"
    SEQUENCE_SIDECAR=1
    ;;
esac

{
  echo "ABLATION_MODE=$MODE"
  echo "SCENARIO=baseline"
  echo "RUN_LIVE=$RUN_LIVE"
  if [[ "$RUN_LIVE" == "1" ]]; then
    echo "MCP_HOST_CMD=$MCP_HOST_CMD"
  fi
  echo "WRAP_POLICY=$FIX_DIR/policies/baseline_wrap.yaml"
} > "$MODE_DIR/baseline.log"

ABLATION_MODE="$MODE" \
RUN_LIVE="$RUN_LIVE" \
MCP_HOST_CMD="$MCP_HOST_CMD" \
MCP_HOST_ARGS="$MCP_HOST_ARGS" \
ASSAY_CMD="$ASSAY_CMD" \
RUNS_ATTACK="${RUNS_ATTACK:-2}" \
RUNS_LEGIT="${RUNS_LEGIT:-1}" \
RUN_SET="${RUN_SET:-deterministic}" \
bash "$ROOT/scripts/ci/exp-mcp-fragmented-ipi/run_baseline.sh" "$MODE_DIR" >> "$MODE_DIR/baseline.log" 2>&1

{
  echo "ABLATION_MODE=$MODE"
  echo "SCENARIO=protected"
  echo "RUN_LIVE=$RUN_LIVE"
  echo "SEQUENCE_SIDECAR=$SEQUENCE_SIDECAR"
  if [[ "$SEQUENCE_SIDECAR" == "1" ]]; then
    echo "SIDECAR=enabled"
  else
    echo "SIDECAR=disabled"
  fi
  echo "ASSAY_POLICY=$ASSAY_POLICY"
  if [[ "$RUN_LIVE" == "1" ]]; then
    echo "MCP_HOST_CMD=$MCP_HOST_CMD"
  fi
  echo "SEQUENCE_POLICY_FILE=$SEQUENCE_POLICY_FILE"
} > "$MODE_DIR/protected.log"

ABLATION_MODE="$MODE" \
RUN_LIVE="$RUN_LIVE" \
SEQUENCE_SIDECAR="$SEQUENCE_SIDECAR" \
ASSAY_POLICY="$ASSAY_POLICY" \
SEQUENCE_POLICY_FILE="$SEQUENCE_POLICY_FILE" \
MCP_HOST_CMD="$MCP_HOST_CMD" \
MCP_HOST_ARGS="$MCP_HOST_ARGS" \
ASSAY_CMD="$ASSAY_CMD" \
RUNS_ATTACK="${RUNS_ATTACK:-2}" \
RUNS_LEGIT="${RUNS_LEGIT:-1}" \
RUN_SET="${RUN_SET:-deterministic}" \
bash "$ROOT/scripts/ci/exp-mcp-fragmented-ipi/run_protected.sh" "$MODE_DIR" >> "$MODE_DIR/protected.log" 2>&1

python3 "$ROOT/scripts/ci/exp-mcp-fragmented-ipi/score_runs.py" \
  "$MODE_DIR/baseline_attack.jsonl" \
  "$MODE_DIR/baseline_legit.jsonl" \
  "$MODE_DIR/protected_attack.jsonl" \
  "$MODE_DIR/protected_legit.jsonl" > "$MODE_DIR/summary.json"

echo "[ablation] mode=$MODE done"
