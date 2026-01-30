#!/usr/bin/env bash
# Blessed e2e benchmark: Hyperfine on assay run/ci with warmup, JSON export, median+p95.
# Usage: from repo root, ./scripts/perf_e2e.sh [small|file_backed|ci]
# Requires: hyperfine (https://github.com/sharkdp/hyperfine), cargo build
# Output: --export-json to PERF_E2E_JSON (default: perf_e2e_results.json); median/p95 from JSON.
set -e

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"
ASSAY="${ASSAY:-$REPO_ROOT/target/debug/assay}"
if [ -x "$REPO_ROOT/target/release/assay" ]; then
  ASSAY="$REPO_ROOT/target/release/assay"
fi
PERF_DIR="$REPO_ROOT/tests/fixtures/perf"
OUT_JSON="${PERF_E2E_JSON:-$REPO_ROOT/perf_e2e_results.json}"
WARMUP="${PERF_E2E_WARMUP:-1}"
RUNS="${PERF_E2E_RUNS:-10}"

if ! command -v hyperfine >/dev/null 2>&1; then
  echo "hyperfine not found. Install: https://github.com/sharkdp/hyperfine"
  exit 1
fi
if [ ! -x "$ASSAY" ]; then
  echo "Build assay first: cargo build (or cargo build --release)"
  exit 1
fi

scenario="${1:-small}"
case "$scenario" in
  small)
    # Blessed small: :memory:, no warmup of DB
    CMD="\"$ASSAY\" run --config \"$PERF_DIR/eval_small.yaml\" --trace-file \"$PERF_DIR/trace_small.jsonl\" --db :memory:"
    ;;
  file_backed)
    # File-backed: fresh DB per run (prepare clears it)
    DB_PATH="${TMPDIR:-/tmp}/perf_e2e_bench.db"
    CMD="\"$ASSAY\" run --config \"$PERF_DIR/eval_small.yaml\" --trace-file \"$PERF_DIR/trace_small.jsonl\" --db \"$DB_PATH\""
    PREPARE="rm -f \"$DB_PATH\" \"${DB_PATH}-shm\" \"${DB_PATH}-wal\""
    ;;
  ci)
    # assay ci (same as small but with ci command; use small fixtures)
    CMD="\"$ASSAY\" ci --config \"$PERF_DIR/eval_small.yaml\" --trace-file \"$PERF_DIR/trace_small.jsonl\""
    ;;
  *)
    echo "Usage: $0 [small|file_backed|ci]"
    exit 1
    ;;
esac

echo "perf_e2e: scenario=$scenario warmup=$WARMUP runs=$RUNS out=$OUT_JSON"
if [ -n "$PREPARE" ]; then
  hyperfine --warmup "$WARMUP" --runs "$RUNS" --export-json "$OUT_JSON" \
    --prepare "$PREPARE" \
    "$CMD"
else
  hyperfine --warmup "$WARMUP" --runs "$RUNS" --export-json "$OUT_JSON" \
    "$CMD"
fi

echo "Results written to $OUT_JSON"
if command -v jq >/dev/null 2>&1 && [ -f "$OUT_JSON" ]; then
  # Hyperfine JSON: .results[0].median (seconds), .results[0].times (array in seconds)
  median_s=$(jq -r '.results[0].median // empty' "$OUT_JSON")
  if [ -n "$median_s" ]; then
    median_ms=$(echo "$median_s * 1000" | bc 2>/dev/null || echo "0")
    echo "median: ${median_ms} ms"
  fi
  # p95 from times: sort and take 95th percentile (index = min(floor(length*0.95), length-1))
  p95_s=$(jq -r '[.results[0].times[]?] | sort | .[([((length * 0.95) | floor), (length - 1)] | min)] // empty' "$OUT_JSON")
  if [ -n "$p95_s" ]; then
    p95_ms=$(echo "$p95_s * 1000" | bc 2>/dev/null || echo "0")
    echo "p95:    ${p95_ms} ms"
  fi
fi
