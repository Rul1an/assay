#!/usr/bin/env bash
# Deterministic smoke check: run the example and assert the per-scenario verdicts + reason codes
# match expected-output.txt. Used by CI and as a self-test for the example.
set -euo pipefail
cd "$(dirname "$0")"

# run.sh writes build progress and any diagnosis to stderr. It is captured rather than discarded:
# discarding it (and letting pipefail abort the pipeline) is why a failing example used to reach CI
# as a bare "Process completed with exit code 1" with nothing to read. Its exit code is also read
# here directly, not through a pipe, so a failure is reported before any output is parsed.
log="$(mktemp)"
trap 'rm -f "$log"' EXIT

status=0
raw="$(./run.sh 2>"$log")" || status=$?
if [ "$status" -ne 0 ]; then
  echo "FAIL: run.sh exited ${status}. Its stderr follows:" >&2
  cat "$log" >&2
  exit 1
fi

actual="$(printf '%s\n' "$raw" | "${PYTHON:-python3}" -c '
import sys, re
for line in sys.stdin:
    m = re.search(r"(DENY|ALLOW).*reason=([a-z_]+)", line)
    if not m:
        continue
    out = f"{m.group(1)} {m.group(2)}"
    if "conformance: mismatched" in line:
        out += " +conformance:mismatched"
    print(out)
')"
expected="$(cat expected-output.txt)"

if [ "$actual" = "$expected" ]; then
  echo "OK: privileged-action-gate verdicts match expected-output.txt"
else
  echo "MISMATCH:"
  diff <(printf '%s\n' "$expected") <(printf '%s\n' "$actual") || true
  exit 1
fi
