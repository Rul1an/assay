#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

RUN_LIVE="${RUN_LIVE:-0}"
if [[ "$RUN_LIVE" != "0" ]]; then
  echo "FAIL: ablation harness currently supports local mock execution only (RUN_LIVE=0)"
  exit 2
fi

ART_DIR="$ROOT/target/exp-mcp-fragmented-ipi-ablation/test"
FIX_DIR="$ROOT/scripts/ci/fixtures/exp-mcp-fragmented-ipi"
rm -rf "$ART_DIR"
mkdir -p "$ART_DIR"

cargo build -q -p assay-cli -p assay-mcp-server

echo "[test] RUN_LIVE=$RUN_LIVE"
echo "[test] running all modes"
for mode in wrap_only sequence_only combined; do
  RUNS_ATTACK=2 RUNS_LEGIT=1 RUN_SET=deterministic \
    bash "$ROOT/scripts/ci/exp-mcp-fragmented-ipi/ablation/run_variant.sh" "$ART_DIR" "$FIX_DIR" "$mode"
done

echo "[test] aggregate"
python3 "$ROOT/scripts/ci/exp-mcp-fragmented-ipi/ablation/score_ablation.py" \
  --root "$ART_DIR" \
  --out "$ART_DIR/ablation-summary.json"

test -f "$ART_DIR/ablation-summary.json"

echo "[test] done"
