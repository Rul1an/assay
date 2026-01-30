#!/usr/bin/env bash
# Performance assessment: run assay workloads, record wall-clock, cleanup.
# Usage: from repo root, ./scripts/perf_assess.sh
#        FORENSIC=1 ./scripts/perf_assess.sh   # tail-latency forensic mode
# Requires: cargo build (assay binary at ./target/debug/assay or ASSAY=...)
set -e

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"
ASSAY="${ASSAY:-$REPO_ROOT/target/debug/assay}"
PERF_DIR="$REPO_ROOT/tests/fixtures/perf"
TMPDIR=""
RESULTS=""
FORENSIC="${FORENSIC:-0}"

# Forensic mode: more iterations, per-iteration timing, p99, tmpfs comparison
if [ "$FORENSIC" = "1" ]; then
  echo "=== FORENSIC MODE: tail-latency deep dive ==="
  echo ""
fi

cleanup() {
  if [ -n "$TMPDIR" ] && [ -d "$TMPDIR" ]; then
    rm -rf "$TMPDIR"
  fi
  rm -rf "$REPO_ROOT/.assay"
  rm -f "$REPO_ROOT/junit.xml" "$REPO_ROOT/sarif.json" "$REPO_ROOT/run.json"
}

trap cleanup EXIT

if [ ! -x "$ASSAY" ]; then
  echo "Build assay first: cargo build"
  exit 1
fi

run_one() {
  local config="$1"
  local trace="$2"
  local db="${3:-:memory:}"
  local label="$4"
  local start end ms
  start=$(date +%s.%N)
  "$ASSAY" run --config "$config" --trace-file "$trace" --db "$db" 2>&1 | tail -5 || true
  end=$(date +%s.%N)
  ms=$(echo "scale=0; ($end - $start) * 1000 / 1" | bc 2>/dev/null || echo "0")
  echo "${label}: ${ms} ms"
  RESULTS="$RESULTS${label}: ${ms} ms"$'\n'
}

# Run N times with optional per-run DB (for file-backed: pass db_prefix, we use db_prefix_1.db ... db_prefix_n.db)
# Usage: run_n_times config trace db_or_prefix label n [file_backed] [metrics_out_dir]
# If file_backed=1 then db_or_prefix is a path prefix; each run gets a fresh .db file.
# If metrics_out_dir is set, run.json is copied to metrics_out_dir/run_$i.json per run and store_metrics are aggregated (median/p95) at the end.
run_n_times() {
  local config="$1"
  local trace="$2"
  local db="$3"
  local label="$4"
  local n="${5:-20}"
  local file_backed="${6:-0}"
  local metrics_out="${7:-}"
  local statsfile
  statsfile=$(mktemp)
  local i
  for i in $(seq 1 "$n"); do
    local run_db="$db"
    [ "$file_backed" = "1" ] && run_db="${db}_${i}.db"
    start=$(date +%s.%N)
    "$ASSAY" run --config "$config" --trace-file "$trace" --db "$run_db" 2>&1 | tail -3 > /dev/null || true
    end=$(date +%s.%N)
    ms=$(echo "scale=0; ($end - $start) * 1000 / 1" | bc 2>/dev/null || echo "0")
    echo "$ms" >> "$statsfile"
    if [ -n "$metrics_out" ] && [ -f "$REPO_ROOT/run.json" ]; then
      mkdir -p "$metrics_out"
      cp "$REPO_ROOT/run.json" "$metrics_out/run_${i}.json"
    fi
  done
  sort -n "$statsfile" > "${statsfile}.sorted"
  local median p95
  median=$(awk -v n="$n" '{a[NR]=$1} END {print (n%2==1)?a[(n+1)/2]:(a[n/2]+a[n/2+1])/2}' "${statsfile}.sorted")
  p95=$(awk -v n="$n" 'BEGIN{p=int(0.95*n+0.999); if(p<1)p=1} {a[NR]=$1} END {print a[p]}' "${statsfile}.sorted")
  echo "${label} (${n}x): median=${median} ms p95=${p95} ms"
  RESULTS="$RESULTS${label} (${n}x): median=${median} ms p95=${p95} ms"$'\n'
  if [ -n "$metrics_out" ] && [ -d "$metrics_out" ] && command -v jq >/dev/null 2>&1; then
    for key in store_wait_ms store_write_ms sqlite_busy_count; do
      vals=$(for f in "$metrics_out"/run_*.json; do [ -f "$f" ] && jq -r ".store_metrics.${key} // 0" "$f" 2>/dev/null; done | sort -n)
      count=$(echo "$vals" | grep -c . || true)
      if [ "$count" -gt 0 ]; then
        med=$(echo "$vals" | awk -v n="$count" '{a[NR]=$1} END {print (n%2==1)?a[(n+1)/2]:(a[n/2]+a[n/2+1])/2}')
        p95v=$(echo "$vals" | awk -v n="$count" 'BEGIN{p=int(0.95*n+0.999); if(p<1)p=1} {a[NR]=$1} END {print a[p]}')
        echo "  store_metrics.${key}: median=${med} p95=${p95v}"
      fi
    done
    log_frames=$(for f in "$metrics_out"/run_*.json; do [ -f "$f" ] && jq -r '.store_metrics.wal_checkpoint.log_frames // "null"' "$f" 2>/dev/null; done | sort -n)
    cf_count=$(echo "$log_frames" | grep -c . || true)
    if [ "$cf_count" -gt 0 ]; then
      lf_med=$(echo "$log_frames" | awk -v n="$cf_count" '{a[NR]=$1} END {print (n%2==1)?a[(n+1)/2]:(a[n/2]+a[n/2+1])/2}')
      echo "  wal_checkpoint.log_frames: median=${lf_med}"
    fi
  fi
  rm -f "$statsfile" "${statsfile}.sorted"
}

# Forensic run: more iterations, detailed per-iteration output, p99, max, stddev
# Usage: run_forensic config trace db_prefix label n metrics_out
run_forensic() {
  local config="$1"
  local trace="$2"
  local db_prefix="$3"
  local label="$4"
  local n="${5:-50}"
  local metrics_out="$6"
  local statsfile
  statsfile=$(mktemp)
  local raw_log="${metrics_out}/timings.txt"
  mkdir -p "$metrics_out"
  echo "# iteration_ms" > "$raw_log"

  echo ""
  echo "=== Forensic: $label (${n}×) ==="

  local i
  for i in $(seq 1 "$n"); do
    local run_db="${db_prefix}_${i}.db"
    start=$(date +%s.%N)
    "$ASSAY" run --config "$config" --trace-file "$trace" --db "$run_db" 2>&1 | tail -3 > /dev/null || true
    end=$(date +%s.%N)
    ms=$(echo "scale=2; ($end - $start) * 1000" | bc 2>/dev/null || echo "0")
    echo "$ms" >> "$statsfile"
    echo "$ms" >> "$raw_log"
    if [ -f "$REPO_ROOT/run.json" ]; then
      cp "$REPO_ROOT/run.json" "$metrics_out/run_${i}.json"
    fi
    # Progress indicator every 10 runs
    if [ $((i % 10)) -eq 0 ]; then
      echo "  ... $i/$n done (last: ${ms} ms)"
    fi
  done

  # Compute stats
  sort -n "$statsfile" > "${statsfile}.sorted"
  local min max mean median p95 p99 stddev
  min=$(head -1 "${statsfile}.sorted")
  max=$(tail -1 "${statsfile}.sorted")
  median=$(awk -v n="$n" '{a[NR]=$1} END {print (n%2==1)?a[(n+1)/2]:(a[n/2]+a[n/2+1])/2}' "${statsfile}.sorted")
  p95=$(awk -v n="$n" 'BEGIN{p=int(0.95*n+0.999); if(p<1)p=1} {a[NR]=$1} END {print a[p]}' "${statsfile}.sorted")
  p99=$(awk -v n="$n" 'BEGIN{p=int(0.99*n+0.999); if(p<1)p=1} {a[NR]=$1} END {print a[p]}' "${statsfile}.sorted")
  mean=$(awk '{sum+=$1} END {printf "%.2f", sum/NR}' "${statsfile}.sorted")
  stddev=$(awk -v m="$mean" '{sum+=($1-m)^2} END {printf "%.2f", sqrt(sum/NR)}' "${statsfile}.sorted")

  echo ""
  echo "  Results:"
  echo "    min:    ${min} ms"
  echo "    max:    ${max} ms"
  echo "    mean:   ${mean} ms"
  echo "    median: ${median} ms"
  echo "    p95:    ${p95} ms"
  echo "    p99:    ${p99} ms"
  echo "    stddev: ${stddev} ms"
  echo "    tail_ratio (p99/median): $(echo "scale=2; $p99 / $median" | bc 2>/dev/null || echo "N/A")"

  # Identify outliers (> 2× median)
  local outlier_threshold outlier_count
  outlier_threshold=$(echo "scale=2; $median * 2" | bc 2>/dev/null || echo "999999")
  outlier_count=$(awk -v t="$outlier_threshold" '$1 > t' "${statsfile}.sorted" | wc -l | tr -d ' ')
  if [ "$outlier_count" -gt 0 ]; then
    echo "    ⚠️  outliers (>2× median): $outlier_count runs"
  fi

  # Store metrics aggregation
  if command -v jq >/dev/null 2>&1; then
    echo ""
    echo "  Store metrics:"
    for key in store_wait_ms store_write_ms sqlite_busy_count; do
      vals=$(for f in "$metrics_out"/run_*.json; do [ -f "$f" ] && jq -r ".store_metrics.${key} // 0" "$f" 2>/dev/null; done | sort -n)
      count=$(echo "$vals" | grep -c . || true)
      if [ "$count" -gt 0 ]; then
        med=$(echo "$vals" | awk -v n="$count" '{a[NR]=$1} END {print (n%2==1)?a[(n+1)/2]:(a[n/2]+a[n/2+1])/2}')
        maxv=$(echo "$vals" | tail -1)
        p99v=$(echo "$vals" | awk -v n="$count" 'BEGIN{p=int(0.99*n+0.999); if(p<1)p=1} {a[NR]=$1} END {print a[p]}')
        echo "    ${key}: median=${med} p99=${p99v} max=${maxv}"
      fi
    done
  fi

  RESULTS="$RESULTS${label}: median=${median} p95=${p95} p99=${p99} max=${max} ms"$'\n'
  rm -f "$statsfile" "${statsfile}.sorted"
}

# Forensic tmpfs comparison: same workload on tmpfs vs real disk
run_forensic_tmpfs_compare() {
  local config="$1"
  local trace="$2"
  local label="$3"
  local n="${4:-20}"

  echo ""
  echo "=== Forensic: $label — tmpfs vs disk comparison ==="

  # Disk run
  local disk_dir="$TMPDIR/forensic_disk"
  mkdir -p "$disk_dir"
  run_forensic "$config" "$trace" "$disk_dir/db" "${label}_disk" "$n" "$disk_dir"

  # tmpfs run (if available on Linux)
  if [ -d "/dev/shm" ]; then
    local tmpfs_dir="/dev/shm/assay_forensic_$$"
    mkdir -p "$tmpfs_dir"
    run_forensic "$config" "$trace" "$tmpfs_dir/db" "${label}_tmpfs" "$n" "$tmpfs_dir"
    rm -rf "$tmpfs_dir"
  else
    echo "  (tmpfs /dev/shm not available, skipping tmpfs comparison)"
  fi
}

# --- Small (committed fixtures) ---
echo "=== Perf assessment (wall-clock) ==="
run_one "$PERF_DIR/eval_small.yaml" "$PERF_DIR/trace_small.jsonl" ":memory:" "small_cold"

# --- Generate medium set (30 episodes, 15 tests) in temp ---
TMPDIR=$(mktemp -d)
MEDIUM_TRACE="$TMPDIR/trace_medium.jsonl"
MEDIUM_EVAL="$TMPDIR/eval_medium.yaml"
printf 'configVersion: 1\nsuite: perf_medium\nmodel: trace\nsettings:\n  cache: false\n  parallel: 4\ntests:\n' > "$MEDIUM_EVAL"
for i in $(seq 1 30); do
  printf '  - id: m%d\n    input: { prompt: "m%d" }\n    expected: { type: regex_match, pattern: "ok", flags: ["i"] }\n' "$i" "$i" >> "$MEDIUM_EVAL"
done
for i in $(seq 1 30); do
  t0=$((i * 1000)); t1=$((i * 1000 + 100)); t2=$((i * 1000 + 200))
  printf '{"type":"episode_start","episode_id":"em%d","timestamp":%d,"input":{"prompt":"m%d"}}\n' "$i" "$t0" "$i" >> "$MEDIUM_TRACE"
  printf '{"type":"step","episode_id":"em%d","step_id":"s1","idx":0,"timestamp":%d,"kind":"llm","content":"ok"}\n' "$i" "$t1" >> "$MEDIUM_TRACE"
  printf '{"type":"episode_end","episode_id":"em%d","timestamp":%d,"final_output":"ok"}\n' "$i" "$t2" >> "$MEDIUM_TRACE"
done
run_one "$MEDIUM_EVAL" "$MEDIUM_TRACE" ":memory:" "medium_cold"

# --- File-backed WAL run (20x) with median/p95 ---
run_n_times "$PERF_DIR/eval_small.yaml" "$PERF_DIR/trace_small.jsonl" "$TMPDIR/file_wal_small" "small_file_backed_20x" 20 1

# --- Large set (100 episodes, 50 tests) ---
LARGE_TRACE="$TMPDIR/trace_large.jsonl"
LARGE_EVAL="$TMPDIR/eval_large.yaml"
printf 'configVersion: 1\nsuite: perf_large\nmodel: trace\nsettings:\n  cache: false\n  parallel: 4\ntests:\n' > "$LARGE_EVAL"
for i in $(seq 1 50); do
  printf '  - id: l%d\n    input: { prompt: "l%d" }\n    expected: { type: must_contain, must_contain: ["x"] }\n' "$i" "$i" >> "$LARGE_EVAL"
done
for i in $(seq 1 50); do
  t0=$((i * 1000)); t1=$((i * 1000 + 100)); t2=$((i * 1000 + 200))
  printf '{"type":"episode_start","episode_id":"el%d","timestamp":%d,"input":{"prompt":"l%d"}}\n' "$i" "$t0" "$i" >> "$LARGE_TRACE"
  printf '{"type":"step","episode_id":"el%d","step_id":"s1","idx":0,"timestamp":%d,"kind":"llm","content":"x"}\n' "$i" "$t1" >> "$LARGE_TRACE"
  printf '{"type":"episode_end","episode_id":"el%d","timestamp":%d,"final_output":"x"}\n' "$i" "$t2" >> "$LARGE_TRACE"
done
run_one "$LARGE_EVAL" "$LARGE_TRACE" ":memory:" "large_cold"

# --- Write-heavy worst-case: many tool_calls + large payloads per episode, many result rows ---
WORST_TRACE="$TMPDIR/trace_worst.jsonl"
WORST_EVAL="$TMPDIR/eval_worst.yaml"
# 12 episodes, each with 8 tool_calls + ~400B args/result; 12 tests (deterministic-only store stress)
PAYLOAD='{"data":"'$(printf 'x%.0s' {1..380})'"}'
printf 'configVersion: 1\nsuite: perf_worst\nmodel: trace\nsettings:\n  cache: false\n  parallel: 4\ntests:\n' > "$WORST_EVAL"
for i in $(seq 1 12); do
  printf '  - id: w%d\n    input: { prompt: "w%d" }\n    expected: { type: sequence_valid, rules: [{ type: require, tool: tc_a }] }\n' "$i" "$i" >> "$WORST_EVAL"
done
for i in $(seq 1 12); do
  t0=$((i * 10000)); t1=$((t0 + 50)); t2=$((t0 + 100))
  printf '{"type":"episode_start","episode_id":"ew%d","timestamp":%d,"input":{"prompt":"w%d"}}\n' "$i" "$t0" "$i" >> "$WORST_TRACE"
  printf '{"type":"step","episode_id":"ew%d","step_id":"s1","idx":0,"timestamp":%d,"kind":"llm","content":"call"}\n' "$i" "$t1" >> "$WORST_TRACE"
  for j in $(seq 0 7); do
    ts=$((t0 + 55 + j * 5))
    printf '{"type":"tool_call","episode_id":"ew%d","step_id":"s1","timestamp":%d,"tool_name":"tc_a","call_index":%d,"args":%s,"result":%s}\n' "$i" "$ts" "$j" "$PAYLOAD" "$PAYLOAD" >> "$WORST_TRACE"
  done
  printf '{"type":"episode_end","episode_id":"ew%d","timestamp":%d,"final_output":"ok"}\n' "$i" "$t2" >> "$WORST_TRACE"
done
run_one "$WORST_EVAL" "$WORST_TRACE" ":memory:" "worst_cold_memory"
# File-backed WAL worstcase: 20× for median + p95 (minimum for critical review); save run.json per run and aggregate store_metrics
run_n_times "$WORST_EVAL" "$WORST_TRACE" "$TMPDIR/worst_wal" "worst_file_backed_20x" 20 1 "$TMPDIR/worst_runs"
run_one "$WORST_EVAL" "$WORST_TRACE" "$TMPDIR/worst.db" "worst_file_backed_1x"

# --- Large-payload variant: ~8KB args/result per toolcall (tests serde/I/O and whether checkpointing starts to dominate)
WORST_LARGE_TRACE="$TMPDIR/trace_worst_large.jsonl"
WORST_LARGE_EVAL="$TMPDIR/eval_worst_large.yaml"
# ~8KB payload: 8×1024 x's in {"data":"..."}
_large_data=""
for _ in 1 2 3 4 5 6 7 8; do _large_data="${_large_data}$(printf 'x%.0s' {1..1024})"; done
LARGE_PAYLOAD='{"data":"'"${_large_data}"'"}'
printf 'configVersion: 1\nsuite: perf_worst_large\nmodel: trace\nsettings:\n  cache: false\n  parallel: 4\ntests:\n' > "$WORST_LARGE_EVAL"
for i in $(seq 1 12); do
  printf '  - id: wl%d\n    input: { prompt: "wl%d" }\n    expected: { type: sequence_valid, rules: [{ type: require, tool: tc_a }] }\n' "$i" "$i" >> "$WORST_LARGE_EVAL"
done
for i in $(seq 1 12); do
  t0=$((i * 10000)); t1=$((t0 + 50)); t2=$((t0 + 100))
  printf '{"type":"episode_start","episode_id":"ewl%d","timestamp":%d,"input":{"prompt":"wl%d"}}\n' "$i" "$t0" "$i" >> "$WORST_LARGE_TRACE"
  printf '{"type":"step","episode_id":"ewl%d","step_id":"s1","idx":0,"timestamp":%d,"kind":"llm","content":"call"}\n' "$i" "$t1" >> "$WORST_LARGE_TRACE"
  for j in $(seq 0 7); do
    ts=$((t0 + 55 + j * 5))
    printf '{"type":"tool_call","episode_id":"ewl%d","step_id":"s1","timestamp":%d,"tool_name":"tc_a","call_index":%d,"args":%s,"result":%s}\n' "$i" "$ts" "$j" "$LARGE_PAYLOAD" "$LARGE_PAYLOAD" >> "$WORST_LARGE_TRACE"
  done
  printf '{"type":"episode_end","episode_id":"ewl%d","timestamp":%d,"final_output":"ok"}\n' "$i" "$t2" >> "$WORST_LARGE_TRACE"
done
run_one "$WORST_LARGE_EVAL" "$WORST_LARGE_TRACE" ":memory:" "worst_large_payload_memory"
run_n_times "$WORST_LARGE_EVAL" "$WORST_LARGE_TRACE" "$TMPDIR/worst_large" "worst_large_payload_file_5x" 5 1 "$TMPDIR/worst_large_runs"

# --- Parallel matrix: worstcase file-backed with parallel 1, 4, 8, 16 (5× each to see where p95/store_wait/sqlite_busy knikt)
echo "=== Parallel matrix (worstcase file-backed, 5× per parallel) ==="
for P in 1 4 8 16; do
  WORST_EVAL_P="$TMPDIR/eval_worst_p${P}.yaml"
  printf 'configVersion: 1\nsuite: perf_worst\nmodel: trace\nsettings:\n  cache: false\n  parallel: %s\ntests:\n' "$P" > "$WORST_EVAL_P"
  for i in $(seq 1 12); do
    printf '  - id: w%d\n    input: { prompt: "w%d" }\n    expected: { type: sequence_valid, rules: [{ type: require, tool: tc_a }] }\n' "$i" "$i" >> "$WORST_EVAL_P"
  done
  run_n_times "$WORST_EVAL_P" "$WORST_TRACE" "$TMPDIR/worst_matrix_p${P}" "worst_file_backed_parallel${P}_5x" 5 1 "$TMPDIR/worst_matrix_p${P}_runs"
done

# --- Warm run (same DB): small again with persisted DB ---
DB_FILE="$TMPDIR/perf.db"
run_one "$PERF_DIR/eval_small.yaml" "$PERF_DIR/trace_small.jsonl" "$DB_FILE" "small_warm_run1"
run_one "$PERF_DIR/eval_small.yaml" "$PERF_DIR/trace_small.jsonl" "$DB_FILE" "small_warm_run2"

echo ""
echo "=== Summary ==="
echo "$RESULTS"

# --- Forensic mode: deep dive on tail latency ---
if [ "$FORENSIC" = "1" ]; then
  echo ""
  echo "=============================================="
  echo "=== FORENSIC TAIL-LATENCY ANALYSIS         ==="
  echo "=============================================="

  # Forensic run on worst file-backed (the 120ms p95 culprit)
  run_forensic "$WORST_EVAL" "$WORST_TRACE" "$TMPDIR/forensic_worst" "worst_file_backed" 50 "$TMPDIR/forensic_worst_data"

  # Forensic run on large-payload variant
  run_forensic "$WORST_LARGE_EVAL" "$WORST_LARGE_TRACE" "$TMPDIR/forensic_large" "worst_large_payload" 30 "$TMPDIR/forensic_large_data"

  # tmpfs comparison (Linux only)
  if [ -d "/dev/shm" ]; then
    run_forensic_tmpfs_compare "$WORST_EVAL" "$WORST_TRACE" "worst_tmpfs_vs_disk" 20
  fi

  echo ""
  echo "=== Forensic raw data saved to: $TMPDIR/forensic_*_data/ ==="
  echo "    (per-iteration timings in timings.txt, run.json per iteration)"
fi
