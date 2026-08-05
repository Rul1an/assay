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

echo "ABLATION_MODE=$ABLATION_MODE"
echo "RUN_LIVE=$RUN_LIVE"
case "$RUN_LIVE" in
  0) ;;
  1)
    : "${MCP_HOST_CMD:?MCP_HOST_CMD is required for RUN_LIVE=1}"
    echo "MCP_HOST_CMD=$MCP_HOST_CMD"
    ;;
  *)
    echo "FAIL: RUN_LIVE must be 0 or 1"
    exit 2
    ;;
esac

# Build rather than assert existence: an existence test passes against a stale artifact, and
# these summaries get published in docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-*-RESULTS.md against
# a git SHA, so a stale binary attributes a measurement to source that never ran it. Cargo
# re-checks rather than rebuilds on an up-to-date tree, so callers that already build pay ~3s.
# assay-cli only: RUN_LIVE=1 drives $ASSAY_CMD instead, and baseline never reaches the
# assay-mcp-server sequence guard (drive_fragmented_ipi.py spawns it only for --mode protected
# with --sequence-policy-root). --manifest-path because this script deliberately never cd's.
if [[ "$RUN_LIVE" == "0" ]]; then
  cargo build -q --manifest-path "$ROOT/Cargo.toml" -p assay-cli
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
