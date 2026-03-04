#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

export RUN_LIVE=0

OUT="artifacts/exp_mcp_fragmented_ipi_cross_session_decay_kplus"
rm -rf "$OUT"
mkdir -p "$OUT"

if [[ "${SKIP_CARGO_BUILD:-0}" != "1" ]]; then
  cargo build -q -p assay-cli -p assay-mcp-server
fi

run_mode_decay() {
  local mode="$1"
  local decay="$2"
  local decay_dir="$OUT/decay_runs_${decay}"

  echo "[test] mode=$mode decay_runs=$decay"
  MODE="$mode" DECAY_RUNS="$decay" OUT_DIR="$decay_dir" \
    bash scripts/ci/exp-mcp-fragmented-ipi/cross_session/run_cross_session_decay.sh

  local sess_dir="$decay_dir/sessions/${mode}/decay_runs_${decay}"
  python3 scripts/ci/exp-mcp-fragmented-ipi/score_cross_session_decay.py \
    --session-dir "$sess_dir" \
    --out "$decay_dir/${mode}-summary.json"
}

for decay in 1 2 3; do
  run_mode_decay wrap_only "$decay"
  run_mode_decay sequence_only "$decay"
  run_mode_decay combined "$decay"
done

echo "[test] sanity: summaries exist"
for decay in 1 2 3; do
  test -f "$OUT/decay_runs_${decay}/wrap_only-summary.json"
  test -f "$OUT/decay_runs_${decay}/sequence_only-summary.json"
  test -f "$OUT/decay_runs_${decay}/combined-summary.json"
done

echo "[test] directional asserts"
python3 - <<'PY'
import json
from pathlib import Path

root = Path("artifacts/exp_mcp_fragmented_ipi_cross_session_decay_kplus")

def load(decay: int, mode: str):
    path = root / f"decay_runs_{decay}" / f"{mode}-summary.json"
    with path.open(encoding="utf-8") as handle:
        return json.load(handle)

for decay in (1, 2, 3):
    wrap = load(decay, "wrap_only")
    sequence = load(decay, "sequence_only")
    combined = load(decay, "combined")

    for key in ("success_k_plus_1", "success_k_plus_2", "success_k_plus_3"):
        wrap_value = wrap.get(key)
        seq_value = sequence.get(key)
        comb_value = combined.get(key)

        if wrap_value is not None and wrap_value is not True:
            raise SystemExit(f"FAIL: wrap_only expected to allow {key} for decay={decay}")
        if seq_value is not None and seq_value is not False:
            raise SystemExit(f"FAIL: sequence_only expected to block {key} for decay={decay}")
        if comb_value is not None and comb_value is not False:
            raise SystemExit(f"FAIL: combined expected to block {key} for decay={decay}")

    if wrap["same_session_control_any_sink_canary"] is not True:
        raise SystemExit(f"FAIL: wrap_only expected to allow same-session control for decay={decay}")
    if sequence["same_session_control_any_sink_canary"] is not False:
        raise SystemExit(f"FAIL: sequence_only expected to block same-session control for decay={decay}")
    if combined["same_session_control_any_sink_canary"] is not False:
        raise SystemExit(f"FAIL: combined expected to block same-session control for decay={decay}")

    for summary in (sequence, combined):
        legit = summary["legit_control"]
        if legit["cross_session_window_active"] is not False:
            raise SystemExit(f"FAIL: legit control must be outside the active decay window for decay={decay}")
        if legit["false_positive"] is not False:
            raise SystemExit(f"FAIL: legit control should remain allowed for decay={decay}")

print("ok")
PY

echo "[test] done"
