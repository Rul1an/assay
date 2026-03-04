#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
cd "$ROOT"

DECAY_RUNS="${DECAY_RUNS:-1}"
MODE="${MODE:-sequence_only}"
RUN_LIVE="${RUN_LIVE:-0}"
OUT_DIR="${OUT_DIR:-artifacts/exp_mcp_fragmented_ipi_cross_session_decay}"
FIX_DIR="$ROOT/scripts/ci/fixtures/exp-mcp-fragmented-ipi"

export RUN_LIVE
export EXPERIMENT_VARIANT="cross_session_decay"
export DECAY_RUNS

case "$MODE" in
  wrap_only)
    WRAP_POLICY="$FIX_DIR/policies/ablation_wrap_only.yaml"
    USE_SEQUENCE=0
    ;;
  sequence_only)
    WRAP_POLICY="$FIX_DIR/policies/ablation_sequence_only.yaml"
    USE_SEQUENCE=1
    ;;
  combined)
    WRAP_POLICY="$FIX_DIR/policies/ablation_combined.yaml"
    USE_SEQUENCE=1
    ;;
  *)
    echo "FAIL: unknown mode: $MODE"
    exit 2
    ;;
esac

SESSION_DIR="$OUT_DIR/sessions/${MODE}/decay_runs_${DECAY_RUNS}"
STATE_FILE="$SESSION_DIR/state/session_guard_state.json"
CONTROL_STATE_FILE="$SESSION_DIR/state/legit_control_state.json"
SAME_SESSION_STATE_FILE="$SESSION_DIR/state/same_session_state.json"
rm -rf "$SESSION_DIR"
mkdir -p "$SESSION_DIR"

echo "[runner] mode=$MODE decay_runs=$DECAY_RUNS run_live=$RUN_LIVE"
echo "[runner] state_file=$STATE_FILE session_dir=$SESSION_DIR"

run_session() {
  local label="$1"
  local session_index="$2"
  local phase="$3"
  local scenario="$4"
  local state_file="$5"
  local call_output_dir="$SESSION_DIR/$label"
  local log_file="$SESSION_DIR/${label}.log"
  local jsonl_file="$SESSION_DIR/${label}.jsonl"
  local args=(
    --repo-root "$ROOT"
    --fixture-root "$FIX_DIR"
    --wrap-policy "$WRAP_POLICY"
    --output-dir "$call_output_dir"
    --output-jsonl "$jsonl_file"
    --mode protected
    --scenario "$scenario"
    --run-set deterministic
    --runs 1
    --run-live "$RUN_LIVE"
    --mcp-host-cmd "${MCP_HOST_CMD:-}"
    --mcp-host-args "${MCP_HOST_ARGS:-}"
    --assay-cmd "${ASSAY_CMD:-assay}"
    --ablation-mode "$MODE"
    --experiment-variant cross_session_decay
    --cross-session-phase "$phase"
    --cross-session-state-file "$state_file"
    --decay-runs "$DECAY_RUNS"
    --session-index "$session_index"
  )

  if [[ "$USE_SEQUENCE" == "1" ]]; then
    args+=(--sequence-policy-root "$FIX_DIR/policies" --sequence-policy-file fragmented_sequence.yaml)
  fi

  mkdir -p "$call_output_dir"
  python3 "$ROOT/scripts/ci/exp-mcp-fragmented-ipi/drive_fragmented_ipi.py" "${args[@]}" >"$log_file" 2>&1
}

run_session session_read_k 1 read_only attack "$STATE_FILE"
run_session session_sink_k1 2 sink_only attack "$STATE_FILE"

if (( DECAY_RUNS >= 2 )); then
  run_session session_sink_k2 3 sink_only attack "$STATE_FILE"
fi

if (( DECAY_RUNS >= 3 )); then
  run_session session_sink_k3 4 sink_only attack "$STATE_FILE"
fi

run_session session_legit 90 legit_control legit "$CONTROL_STATE_FILE"
run_session session_same_session_control 91 same_session_control attack "$SAME_SESSION_STATE_FILE"

echo "[runner] done"
