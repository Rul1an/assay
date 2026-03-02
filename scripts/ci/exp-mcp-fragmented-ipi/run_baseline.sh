#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
OUT_DIR="${1:-$ROOT/target/exp-mcp-fragmented-ipi/baseline}"
RUNS_ATTACK="${RUNS_ATTACK:-2}"
RUNS_LEGIT="${RUNS_LEGIT:-1}"
RUN_SET="${RUN_SET:-deterministic}"
FIXTURE_ROOT="$ROOT/scripts/ci/fixtures/exp-mcp-fragmented-ipi"
POLICY="$FIXTURE_ROOT/policies/baseline_wrap.yaml"
mkdir -p "$OUT_DIR"

test -x "$ROOT/target/debug/assay" || { echo "Missing $ROOT/target/debug/assay; build assay-cli first"; exit 1; }
test -x "$ROOT/target/debug/assay-mcp-server" || { echo "Missing $ROOT/target/debug/assay-mcp-server; build assay-mcp-server first"; exit 1; }

python3 "$ROOT/scripts/ci/exp-mcp-fragmented-ipi/drive_fragmented_ipi.py" \
  --repo-root "$ROOT" \
  --fixture-root "$FIXTURE_ROOT" \
  --wrap-policy "$POLICY" \
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
  --output-dir "$OUT_DIR" \
  --output-jsonl "$OUT_DIR/baseline_legit.jsonl" \
  --mode baseline \
  --scenario legit \
  --run-set "$RUN_SET" \
  --runs "$RUNS_LEGIT"

python3 "$ROOT/scripts/ci/exp-mcp-fragmented-ipi/score_runs.py" \
  "$OUT_DIR/baseline_attack.jsonl" \
  "$OUT_DIR/baseline_legit.jsonl" > "$OUT_DIR/summary.json"
