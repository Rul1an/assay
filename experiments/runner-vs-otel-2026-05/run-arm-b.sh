#!/usr/bin/env bash
# Arm B (trace only) runner for the runner-vs-otel-2026-05 experiment.
#
# Runs the deterministic OpenAI Agents workload under OpenTelemetry tracing
# without any Runner archive capture. Produces:
#   - runs/<run-id>/trace.json     OTLP/JSON trace export
#   - runs/<run-id>/matrix.json    Field matrix output (archive arm marked absent)
#   - runs/<run-id>/matrix.md      Markdown summary
#
# Arms A and C live on the delegated `assay-bpf-runner` host because they
# require Linux/eBPF/cgroup-v2 capture; see README.md for the dispatch path.

set -euo pipefail

cd "$(dirname "$0")"

EXPERIMENT_ROOT="$PWD"
WORKLOAD_DIR="$EXPERIMENT_ROOT/workload"
COMPARE_DIR="$EXPERIMENT_ROOT/compare"

RUN_ID="${RUN_ID:-run_arm_b_$(date -u +%Y%m%dT%H%M%SZ)}"
RUN_DIR="$EXPERIMENT_ROOT/runs/$RUN_ID"
TRACE_OUT="$RUN_DIR/trace.json"
MATRIX_JSON="$RUN_DIR/matrix.json"
MATRIX_MD="$RUN_DIR/matrix.md"
SYNTHETIC_ARCHIVE_DIR="$COMPARE_DIR/tests/fixtures/archive"

mkdir -p "$RUN_DIR"

echo "==> Arm B: trace-only workload run"
echo "    run_id    = $RUN_ID"
echo "    trace_out = $TRACE_OUT"

(
  cd "$WORKLOAD_DIR"
  if [ ! -d node_modules ]; then
    echo "==> npm install (one-time)"
    npm install --no-audit --no-fund --ignore-scripts
  fi
  if [ ! -d dist ] || [ src -nt dist ]; then
    echo "==> tsc"
    npx tsc -p tsconfig.json
  fi
  node dist/workload.js --run-id "$RUN_ID" --trace-out "$TRACE_OUT"
)

echo "==> compare against synthetic archive (Arm B has no real archive)"
echo "    using $SYNTHETIC_ARCHIVE_DIR for the archive side to exercise the comparator"
python3 "$COMPARE_DIR/compare.py" \
  --archive "$SYNTHETIC_ARCHIVE_DIR" \
  --trace "$TRACE_OUT" \
  --out-json "$MATRIX_JSON" \
  --out-md "$MATRIX_MD"

echo
echo "==> Arm B run complete:"
echo "    trace:  $TRACE_OUT"
echo "    matrix: $MATRIX_JSON"
echo "    md:     $MATRIX_MD"
echo
echo "Arm B intentionally pairs the trace with a synthetic fixture archive to"
echo "exercise the comparator end-to-end. For a real (Runner archive, trace)"
echo "pair, dispatch Arm C on the delegated host; see README.md."
