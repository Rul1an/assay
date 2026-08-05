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

# Build what this run executes, rather than asserting a binary exists: an existence test passes
# against a stale artifact, and these summaries get published in
# docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-*-RESULTS.md against a git SHA, so a stale binary
# attributes a measurement to source that never ran it. Warm cost is ~0.3s.
#
# assay-cli is built on BOTH values of RUN_LIVE. RUN_LIVE=1 selects a real MCP host, not a
# foreign assay: the live rerun docs set ASSAY_CMD to a target/debug/assay path, and
# EXPERIMENT-MCP-FRAGMENTED-IPI-ABLATION-2026Q1-RERUN.md asks the operator by hand to "ensure
# ASSAY_CMD points to the freshly built target/debug/assay". This discharges that obligation
# instead of restating it.
#
# assay-mcp-server is deliberately absent: drive_fragmented_ipi.py spawns the sequence guard
# only for --mode protected with --sequence-policy-root, and baseline passes neither.
#
# --target-dir pins the output to where the driver looks (repo_root/"target/debug/..."). Without
# it a CARGO_TARGET_DIR -- which AGENTS.md tells worktree owners to set -- would send the build
# somewhere the driver never opens. --manifest-path because this script never cd's.
if [[ "${SKIP_CARGO_BUILD:-0}" != "1" ]]; then
  cargo build -q --manifest-path "$ROOT/Cargo.toml" --target-dir "$ROOT/target" -p assay-cli
fi

# Say what this build does not cover rather than implying it covers everything.
if [[ "$RUN_LIVE" == "1" && "$ASSAY_CMD" != "$ROOT/target/debug/assay" ]]; then
  echo "NOTE: ASSAY_CMD is not this worktree's target/debug/assay; that binary's freshness is the caller's"
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
