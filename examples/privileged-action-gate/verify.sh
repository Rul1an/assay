#!/usr/bin/env bash
# Deterministic smoke check: run the example and assert the per-scenario verdicts + reason codes
# match expected-output.txt. Used by CI and as a self-test for the example.
set -euo pipefail
cd "$(dirname "$0")"

actual="$(./run.sh 2>/dev/null | "${PYTHON:-python3}" -c '
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
