#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

POLICY="schemas/closure_release_policy_v1.json"

run_case() {
  local mode="$1"
  local json="$2"
  local expect="$3"

  set +e
  ASSAY_CLOSURE_RELEASE_TEST_MODE=1 \
  MODE="$mode" \
  POLICY="$POLICY" \
  ASSAY_CLOSURE_RELEASE_LOCAL_JSON="$json" \
  bash scripts/ci/adr025-closure-release.sh
  code=$?
  set -e

  if [[ "$code" -ne "$expect" ]]; then
    echo "FAIL: mode=$mode json=$json expected=$expect got=$code"
    exit 1
  fi
  echo "ok: mode=$mode expected=$expect"
}

run_missing_case() {
  local mode="$1"
  local expect="$2"

  set +e
  ASSAY_CLOSURE_RELEASE_TEST_MODE=1 \
  MODE="$mode" \
  POLICY="$POLICY" \
  ASSAY_CLOSURE_RELEASE_SIMULATE_MISSING_ARTIFACT=1 \
  bash scripts/ci/adr025-closure-release.sh
  code=$?
  set -e

  if [[ "$code" -ne "$expect" ]]; then
    echo "FAIL: missing-artifact mode=$mode expected=$expect got=$code"
    exit 1
  fi
  echo "ok: missing-artifact mode=$mode expected=$expect"
}

echo "[test] attach mode never blocks (0) on policy fail"
run_case "attach" "scripts/ci/fixtures/adr025-i2/closure_report_fail.json" 0

echo "[test] warn mode never blocks (0) on policy fail"
run_case "warn" "scripts/ci/fixtures/adr025-i2/closure_report_fail.json" 0

echo "[test] enforce blocks on low score (1)"
run_case "enforce" "scripts/ci/fixtures/adr025-i2/closure_report_fail.json" 1

echo "[test] enforce passes on high score (0)"
run_case "enforce" "scripts/ci/fixtures/adr025-i2/closure_report_pass.json" 0

echo "[test] schema mismatch is measurement fail (2) in enforce"
run_case "enforce" "scripts/ci/fixtures/adr025-i2/closure_report_schema_mismatch.json" 2

echo "[test] violations null treated as empty (enforce pass)"
run_case "enforce" "scripts/ci/fixtures/adr025-i2/closure_report_violations_null.json" 0

echo "[test] violations wrong type is measurement fail (2) in enforce"
run_case "enforce" "scripts/ci/fixtures/adr025-i2/closure_report_violations_wrong_type.json" 2

echo "[test] missing artifact semantics by mode"
run_missing_case "attach" 0
run_missing_case "warn" 0
run_missing_case "enforce" 2

echo "[test] done"
