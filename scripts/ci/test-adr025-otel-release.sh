#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

POLICY="schemas/otel_release_policy_v1.json"

assert_decision_log() {
  local output="$1"
  local expected_code="$2"
  local expected_mode="$3"
  local expected_decision="$4"

  local count
  count="$(printf "%s\n" "$output" | rg -c '"event":"adr025\.otel_release_decision"' || true)"
  if [[ "$count" -ne 1 ]]; then
    echo "FAIL: expected exactly one decision JSON log line, got $count"
    printf "%s\n" "$output"
    exit 1
  fi

  local line
  line="$(printf "%s\n" "$output" | rg '"event":"adr025\.otel_release_decision"' | tail -n 1)"
  if [[ -z "$line" ]]; then
    echo "FAIL: missing decision JSON log line"
    exit 1
  fi

  python3 - <<'PY' "$line" "$expected_code" "$expected_mode" "$expected_decision"
import json
import sys

line, expected_code, expected_mode, expected_decision = sys.argv[1], int(sys.argv[2]), sys.argv[3], sys.argv[4]
obj = json.loads(line)

for key in ("event", "mode", "score", "threshold", "decision", "exit_code"):
    if key not in obj:
        raise SystemExit(f"missing key: {key}")

if obj["event"] != "adr025.otel_release_decision":
    raise SystemExit(f"unexpected event: {obj['event']}")
if obj["mode"] != expected_mode:
    raise SystemExit(f"unexpected mode: {obj['mode']} != {expected_mode}")
if obj["decision"] != expected_decision:
    raise SystemExit(f"unexpected decision: {obj['decision']} != {expected_decision}")
if int(obj["exit_code"]) != expected_code:
    raise SystemExit(f"unexpected exit_code: {obj['exit_code']} != {expected_code}")
PY
}

run_case() {
  local mode="$1"
  local json="$2"
  local expect="$3"
  local expect_decision="$4"

  local output_file
  output_file="$(mktemp)"

  set +e
  ASSAY_OTEL_RELEASE_TEST_MODE=1 \
  MODE="$mode" \
  POLICY="$POLICY" \
  ASSAY_OTEL_RELEASE_LOCAL_JSON="$json" \
  bash scripts/ci/adr025-otel-release.sh >"$output_file" 2>&1
  code=$?
  set -e

  local output
  output="$(cat "$output_file")"
  rm -f "$output_file"

  assert_decision_log "$output" "$code" "$mode" "$expect_decision"

  if [[ "$code" -ne "$expect" ]]; then
    echo "FAIL: mode=$mode json=$json expected=$expect got=$code"
    printf "%s\n" "$output"
    exit 1
  fi

  echo "ok: mode=$mode expected=$expect decision=$expect_decision"
}

run_missing_case() {
  local mode="$1"
  local expect="$2"
  local expect_decision="$3"

  local output_file
  output_file="$(mktemp)"

  set +e
  ASSAY_OTEL_RELEASE_TEST_MODE=1 \
  MODE="$mode" \
  POLICY="$POLICY" \
  ASSAY_OTEL_RELEASE_SIMULATE_MISSING_ARTIFACT=1 \
  bash scripts/ci/adr025-otel-release.sh >"$output_file" 2>&1
  code=$?
  set -e

  local output
  output="$(cat "$output_file")"
  rm -f "$output_file"

  assert_decision_log "$output" "$code" "$mode" "$expect_decision"

  if [[ "$code" -ne "$expect" ]]; then
    echo "FAIL: missing-artifact mode=$mode expected=$expect got=$code"
    printf "%s\n" "$output"
    exit 1
  fi

  echo "ok: missing-artifact mode=$mode expected=$expect decision=$expect_decision"
}

echo "[test] off mode skips"
set +e
off_out="$(MODE=off POLICY="$POLICY" bash scripts/ci/adr025-otel-release.sh 2>&1)"
off_code=$?
set -e
assert_decision_log "$off_out" "$off_code" "off" "skip"
test "$off_code" -eq 0

echo "[test] attach mode remains non-blocking on contract failure"
run_case "attach" "scripts/ci/fixtures/adr025-i3/otel_bridge_report_contract_bad.json" 0 "attach"

echo "[test] warn mode remains non-blocking on contract failure"
run_case "warn" "scripts/ci/fixtures/adr025-i3/otel_bridge_report_contract_bad.json" 0 "warn"

echo "[test] enforce mode passes on valid report"
run_case "enforce" "scripts/ci/fixtures/adr025-i3/otel_bridge_report_contract_ok.json" 0 "pass"

echo "[test] enforce mode fails on schema mismatch"
run_case "enforce" "scripts/ci/fixtures/adr025-i3/otel_bridge_report_schema_bad.json" 2 "measurement_fail"

echo "[test] enforce mode fails on contract violation"
run_case "enforce" "scripts/ci/fixtures/adr025-i3/otel_bridge_report_contract_bad.json" 2 "measurement_fail"

echo "[test] missing artifact semantics by mode"
run_missing_case "attach" 0 "attach"
run_missing_case "warn" 0 "warn"
run_missing_case "enforce" 2 "measurement_fail"

echo "[test] done"
