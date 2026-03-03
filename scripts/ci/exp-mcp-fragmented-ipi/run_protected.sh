#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
OUT_DIR="${1:-$ROOT/target/exp-mcp-fragmented-ipi/protected}"
RUNS_ATTACK="${RUNS_ATTACK:-2}"
RUNS_LEGIT="${RUNS_LEGIT:-1}"
RUN_SET="${RUN_SET:-deterministic}"
FIXTURE_ROOT="$ROOT/scripts/ci/fixtures/exp-mcp-fragmented-ipi"
ABLATION_MODE="${ABLATION_MODE:-protected_default}"
RUN_LIVE="${RUN_LIVE:-0}"
SEQUENCE_SIDECAR="${SEQUENCE_SIDECAR:-1}"
ASSAY_CMD="${ASSAY_CMD:-assay}"
ASSAY_POLICY="${ASSAY_POLICY:-}"
MCP_HOST_CMD="${MCP_HOST_CMD:-}"
MCP_HOST_ARGS="${MCP_HOST_ARGS:-}"
WRAP_POLICY="${ASSAY_POLICY:-$FIXTURE_ROOT/policies/protected_wrap.yaml}"
SEQ_ROOT="$FIXTURE_ROOT/policies"
SEQUENCE_POLICY_FILE="${SEQUENCE_POLICY_FILE:-fragmented_sequence.yaml}"
mkdir -p "$OUT_DIR"

test -x "$ROOT/target/debug/assay" || { echo "Missing $ROOT/target/debug/assay; build assay-cli first"; exit 1; }
test -x "$ROOT/target/debug/assay-mcp-server" || { echo "Missing $ROOT/target/debug/assay-mcp-server; build assay-mcp-server first"; exit 1; }

echo "ABLATION_MODE=$ABLATION_MODE"
echo "RUN_LIVE=$RUN_LIVE"
echo "SEQUENCE_SIDECAR=$SEQUENCE_SIDECAR"
echo "ASSAY_POLICY=$WRAP_POLICY"
if [[ "$SEQUENCE_SIDECAR" == "1" ]]; then
  echo "SIDECAR=enabled"
else
  echo "SIDECAR=disabled"
fi
case "$RUN_LIVE" in
  0) ;;
  1)
    : "${MCP_HOST_CMD:?MCP_HOST_CMD is required for RUN_LIVE=1}"
    test -f "$WRAP_POLICY" || { echo "Measurement error: policy file not found: $WRAP_POLICY"; exit 2; }
    echo "MCP_HOST_CMD=$MCP_HOST_CMD"
    ;;
  *)
    echo "FAIL: RUN_LIVE must be 0 or 1"
    exit 2
    ;;
esac

ATTACK_ARGS=(
  --repo-root "$ROOT"
  --fixture-root "$FIXTURE_ROOT"
  --wrap-policy "$WRAP_POLICY"
  --run-live "$RUN_LIVE"
  --mcp-host-cmd "$MCP_HOST_CMD"
  --mcp-host-args "$MCP_HOST_ARGS"
  --assay-cmd "$ASSAY_CMD"
  --output-dir "$OUT_DIR"
  --output-jsonl "$OUT_DIR/protected_attack.jsonl"
  --mode protected
  --scenario attack
  --run-set "$RUN_SET"
  --runs "$RUNS_ATTACK"
  --ablation-mode "$ABLATION_MODE"
)
LEGIT_ARGS=(
  --repo-root "$ROOT"
  --fixture-root "$FIXTURE_ROOT"
  --wrap-policy "$WRAP_POLICY"
  --run-live "$RUN_LIVE"
  --mcp-host-cmd "$MCP_HOST_CMD"
  --mcp-host-args "$MCP_HOST_ARGS"
  --assay-cmd "$ASSAY_CMD"
  --output-dir "$OUT_DIR"
  --output-jsonl "$OUT_DIR/protected_legit.jsonl"
  --mode protected
  --scenario legit
  --run-set "$RUN_SET"
  --runs "$RUNS_LEGIT"
  --ablation-mode "$ABLATION_MODE"
)

if [[ "$SEQUENCE_SIDECAR" == "1" ]]; then
  ATTACK_ARGS+=(--sequence-policy-root "$SEQ_ROOT" --sequence-policy-file "$SEQUENCE_POLICY_FILE")
  LEGIT_ARGS+=(--sequence-policy-root "$SEQ_ROOT" --sequence-policy-file "$SEQUENCE_POLICY_FILE")
fi

python3 "$ROOT/scripts/ci/exp-mcp-fragmented-ipi/drive_fragmented_ipi.py" "${ATTACK_ARGS[@]}"
python3 "$ROOT/scripts/ci/exp-mcp-fragmented-ipi/drive_fragmented_ipi.py" "${LEGIT_ARGS[@]}"

python3 "$ROOT/scripts/ci/exp-mcp-fragmented-ipi/score_runs.py" \
  "$OUT_DIR/protected_attack.jsonl" \
  "$OUT_DIR/protected_legit.jsonl" > "$OUT_DIR/summary.json"
