#!/usr/bin/env bash
set -euo pipefail

# Parse and assert executed units from a libFuzzer log (-print_final_stats=1).
#
# Exact format citation from LLVM compiler-rt lib/fuzzer/FuzzerLoop.cpp (PrintFinalStats):
#   Printf("stat::number_of_executed_units: %zd\n", TotalNumberOfRuns);
#   Printf("stat::average_exec_per_sec:     %zd\n", ExecsPerSec);
#   Printf("stat::new_units_added:          %zd\n", NumberOfNewUnitsAdded);
#   Printf("stat::slowest_unit_time_sec:    %zd\n", SlowestUnitStartTime);
#   Printf("stat::peak_rss_mb:              %zd\n", GetPeakRSSMb());
#
# In multi-worker runs, each worker process prints its own final stats block.
# We sum the executed units across all matching stat lines so multi-worker
# totals are counted accurately.

LOG_FILE="${1:-}"
TARGET="${TARGET:-${2:-fuzz_target}}"

if [[ -n "$LOG_FILE" && "$LOG_FILE" != "-" ]]; then
  if [[ ! -f "$LOG_FILE" ]]; then
    echo "::error::fuzz log file not found: ${LOG_FILE}" >&2
    exit 1
  fi
  INPUT="$(cat "$LOG_FILE")"
else
  INPUT="$(cat)"
fi

# Strict anchored match for stat::number_of_executed_units: <integer>
matched_lines="$(grep -E '^stat::number_of_executed_units:' <<<"$INPUT" || true)"

if [[ -z "$matched_lines" ]]; then
  echo "::error::no stat::number_of_executed_units line found in fuzz output" >&2
  exit 1
fi

total_units=0

while IFS= read -r line; do
  [[ -z "$line" ]] && continue
  if [[ "$line" =~ ^stat::number_of_executed_units:[[:space:]]*([0-9]+)[[:space:]]*$ ]]; then
    val="${BASH_REMATCH[1]}"
    val=$((10#$val))
    total_units=$((total_units + val))
  else
    echo "::error::malformed stat::number_of_executed_units line: ${line}" >&2
    exit 1
  fi
done <<<"$matched_lines"

if [[ "$total_units" -le 0 ]]; then
  echo "::error::fuzz target ${TARGET} executed 0 units" >&2
  exit 1
fi

echo "ok: fuzz target ${TARGET} executed ${total_units} units"
