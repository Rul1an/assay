#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

export RUN_LIVE=0
export DECAY_RUNS="${DECAY_RUNS:-1}"

OUT="artifacts/exp_mcp_fragmented_ipi_cross_session_decay"
rm -rf "$OUT"
mkdir -p "$OUT"

if [[ "${SKIP_CARGO_BUILD:-0}" != "1" ]]; then
  cargo build -q -p assay-cli -p assay-mcp-server
fi

run_mode() {
  local mode="$1"
  echo "[test] mode=$mode decay_runs=$DECAY_RUNS"
  MODE="$mode" OUT_DIR="$OUT" bash scripts/ci/exp-mcp-fragmented-ipi/cross_session/run_cross_session_decay.sh

  local sess_dir="$OUT/sessions/${mode}/decay_runs_${DECAY_RUNS}"
  python3 scripts/ci/exp-mcp-fragmented-ipi/score_cross_session_decay.py \
    --session-dir "$sess_dir" \
    --out "$OUT/${mode}-summary.json"
}

run_mode wrap_only
run_mode sequence_only
run_mode combined

echo "[test] sanity: summaries exist"
test -f "$OUT/wrap_only-summary.json"
test -f "$OUT/sequence_only-summary.json"
test -f "$OUT/combined-summary.json"

echo "[test] directional asserts"
python3 - <<'PY'
import json

load = lambda p: json.load(open(p, encoding='utf-8'))
w = load('artifacts/exp_mcp_fragmented_ipi_cross_session_decay/wrap_only-summary.json')
s = load('artifacts/exp_mcp_fragmented_ipi_cross_session_decay/sequence_only-summary.json')
c = load('artifacts/exp_mcp_fragmented_ipi_cross_session_decay/combined-summary.json')

if w['success_any_sink_canary'] is not True:
    raise SystemExit('FAIL: wrap_only expected to allow session2 sink attempt')
if s['success_any_sink_canary'] is True:
    raise SystemExit('FAIL: sequence_only expected to block session2 sink attempt')
if c['success_any_sink_canary'] is True:
    raise SystemExit('FAIL: combined expected to block session2 sink attempt')
if w['same_session_control_any_sink_canary'] is not True:
    raise SystemExit('FAIL: wrap_only expected to allow same-session control attempt')
if s['same_session_control_any_sink_canary'] is True:
    raise SystemExit('FAIL: sequence_only expected to block same-session control attempt')
if c['same_session_control_any_sink_canary'] is True:
    raise SystemExit('FAIL: combined expected to block same-session control attempt')

for summary in (s, c):
    if summary['sessions'][2]['cross_session_window_active'] is not False:
        raise SystemExit('FAIL: legit control must run outside the active decay window')
    if summary['sessions'][2]['false_positive'] is not False:
        raise SystemExit('FAIL: legit control should remain allowed once decay window expires')

print('ok')
PY

echo "[test] done"
