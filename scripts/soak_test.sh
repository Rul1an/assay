#!/usr/bin/env bash
set -eo pipefail

ASSAY="${ASSAY:-./target/debug/assay}"
ITERATIONS="${ITERATIONS:-50}"

CONFIG="${CONFIG:-tests/fixtures/contract/pass.yaml}"
TRACE="${TRACE:-goldens.jsonl}" # Not strictly used by pass.yaml but kept for generic usage
EXTRA_ARGS=("$@")

WORKDIR="$(mktemp -d)"
cleanup() { rm -rf "$WORKDIR"; }
trap cleanup EXIT

TIMES="$WORKDIR/times_seconds.txt"
CODES="$WORKDIR/exit_codes.txt"
LOGS_DIR="$WORKDIR/logs"
mkdir -p "$LOGS_DIR"

CMD_BASE=(
  "$ASSAY" run
  --config "$CONFIG"
  --strict
)

echo "== Assay Soak Test =="
echo "assay: $("$ASSAY" --version || true)"
echo "iterations: $ITERATIONS"
echo "cmd: ${CMD_BASE[*]} ${EXTRA_ARGS[*]:-}"
echo

# Warm-up (cache warm)
echo "[warmup] running once to warm cache..."
set +e
start=$(python3 -c 'import time; print(time.time())')
"${CMD_BASE[@]}" "${EXTRA_ARGS[@]}" >"$LOGS_DIR/warmup.out" 2>"$LOGS_DIR/warmup.err"
WARM_CODE=$?
set -e

if [[ "$WARM_CODE" -ne 0 ]]; then
  echo "FATAL: warmup failed with exit code $WARM_CODE"
  echo "---- stderr ----"
  sed -n '1,160p' "$LOGS_DIR/warmup.err"
  exit 90
fi

# Runs
FAILS=0
NONZERO_CODES=0
CACHE_MISS_COUNT=0

for i in $(seq 1 "$ITERATIONS"); do
  OUT="$LOGS_DIR/run_${i}.out"
  ERR="$LOGS_DIR/run_${i}.err"
  TFILE="$WORKDIR/run_${i}.time"

  start=$(python3 -c 'import time; print(time.time())')

  set +e
  "${CMD_BASE[@]}" "${EXTRA_ARGS[@]}" >"$OUT" 2>"$ERR"
  CODE=$?
  set -e

  end=$(python3 -c 'import time; print(time.time())')
  python3 -c "print($end - $start)" > "$TFILE"

  echo "$CODE" >> "$CODES"
  cat "$TFILE" >> "$TIMES"

  if [[ "$CODE" -ne 0 ]]; then
    ((NONZERO_CODES++)) || true
    echo "[run $i] ❌ exit=$CODE"
    # Print first lines to make CI logs useful
    sed -n '1,120p' "$ERR" | sed 's/^/  /'
  else
    # Check for cache hit
    # Since assay only logs [CACHE HIT], absence of it (for a known test) implies miss/live call.
    if grep -Fq "[CACHE HIT]" "$ERR"; then
         echo "[run $i] ✅ exit=0 (cache hit)"
    else
         echo "[run $i] ⚠️  exit=0 (CACHE MISS)"
         ((CACHE_MISS_COUNT++)) || true
    fi
  fi
done

# Stats
COUNT="$(wc -l < "$TIMES" | tr -d ' ')"
SORTED="$WORKDIR/times_sorted.txt"
sort -n "$TIMES" > "$SORTED"

MEAN="$(awk '{s+=$1} END {if (NR>0) printf "%.6f", s/NR; else print "nan"}' "$SORTED")"
MIN="$(head -n 1 "$SORTED")"
MAX="$(tail -n 1 "$SORTED")"
P50="$(awk '{a[NR]=$1} END {print a[int(NR*0.50 - 0.5)]}' "$SORTED")"
P95="$(awk '{a[NR]=$1} END {print a[int(NR*0.95 - 0.5)]}' "$SORTED")"
P99="$(awk '{a[NR]=$1} END {print a[int(NR*0.99 - 0.5)]}' "$SORTED")"

echo
echo "== Summary =="
echo "runs:        $COUNT"
echo "exit!=0:     $NONZERO_CODES"
echo "cache_miss:  $CACHE_MISS_COUNT   (runs with 0 cache hits)"
echo "min(s):      $MIN"
echo "mean(s):     $MEAN"
echo "p50(s):      $P50"
echo "p95(s):      $P95"
echo "p99(s):      $P99"
echo "max(s):      $MAX"

# Hard gates
if [[ "$NONZERO_CODES" -ne 0 ]]; then
  echo "FAIL: non-zero exits detected"
  exit 1
fi

# Cache expectation: after warm-up we expect 0 misses
if [[ "$CACHE_MISS_COUNT" -ne 0 ]]; then
  echo "FAIL: cache misses detected after warm-up (expected 100% hit rate)"
  exit 1
fi

echo "PASS: soak test stable"
