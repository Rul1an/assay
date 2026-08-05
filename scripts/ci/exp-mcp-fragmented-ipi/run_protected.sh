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

# Build what this run executes, rather than asserting a binary exists: an existence test passes
# against a stale artifact, and these summaries get published in
# docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-*-RESULTS.md against a git SHA, so a stale binary
# attributes a measurement to source that never ran it. Warm cost is ~0.3s.
#
# assay-cli is built on BOTH values of RUN_LIVE. RUN_LIVE=1 selects a real MCP host, not a
# foreign assay: the live rerun docs set ASSAY_CMD to a target/debug/assay path, and
# EXPERIMENT-MCP-FRAGMENTED-IPI-ABLATION-2026Q1-RERUN.md asks the operator by hand to "ensure
# ASSAY_CMD points to the freshly built target/debug/assay". Gating this on RUN_LIVE=0 would
# leave the live path -- whose numbers are published as live -- the one with no guarantee, and
# with the sidecar on it would rebuild the guard while measuring it against a stale wrap binary.
#
# assay-mcp-server tracks the sidecar, which is the only thing that reaches the sequence guard.
#
# --target-dir pins the output to where the driver looks (repo_root/"target/debug/..."). Without
# it a CARGO_TARGET_DIR -- which AGENTS.md tells worktree owners to set -- would send the build
# somewhere the driver never opens. --manifest-path because this script never cd's.
BUILD_PKGS=(-p assay-cli)
if [[ "$SEQUENCE_SIDECAR" == "1" ]]; then
  BUILD_PKGS+=(-p assay-mcp-server)
fi
if [[ "${SKIP_CARGO_BUILD:-0}" != "1" ]]; then
  cargo build -q --manifest-path "$ROOT/Cargo.toml" --target-dir "$ROOT/target" "${BUILD_PKGS[@]}"
fi

# Say what this build does not cover rather than implying it covers everything.
if [[ "$RUN_LIVE" == "1" && "$ASSAY_CMD" != "$ROOT/target/debug/assay" ]]; then
  echo "NOTE: ASSAY_CMD is not this worktree's target/debug/assay; that binary's freshness is the caller's"
fi

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
