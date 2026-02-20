#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

OUTDIR="$(mktemp -d)"
trap 'rm -rf "$OUTDIR"' EXIT

POLICY="schemas/closure_release_policy_v1.json"
PASS_JSON="scripts/ci/fixtures/adr025-i2/closure_report_pass.json"
FAIL_JSON="scripts/ci/fixtures/adr025-i2/closure_report_fail.json"
HARD_JSON="scripts/ci/fixtures/adr025-i2/closure_report_hard_violation.json"
INVALID_JSON="scripts/ci/fixtures/adr025-i2/closure_report_invalid.json"

echo "[test] mode=off -> 0"
bash scripts/ci/adr025-closure-release.sh --mode off --policy "$POLICY" --out-dir "$OUTDIR"

echo "[test] mode=attach pass -> 0"
bash scripts/ci/adr025-closure-release.sh --mode attach --policy "$POLICY" --closure-json "$PASS_JSON" --out-dir "$OUTDIR"

echo "[test] mode=attach fail-score -> still 0"
bash scripts/ci/adr025-closure-release.sh --mode attach --policy "$POLICY" --closure-json "$FAIL_JSON" --out-dir "$OUTDIR"

echo "[test] mode=enforce fail-score -> 1"
set +e
bash scripts/ci/adr025-closure-release.sh --mode enforce --policy "$POLICY" --closure-json "$FAIL_JSON" --out-dir "$OUTDIR"
code=$?
set -e
test "$code" -eq 1

echo "[test] mode=enforce hard-violation -> 1"
set +e
bash scripts/ci/adr025-closure-release.sh --mode enforce --policy "$POLICY" --closure-json "$HARD_JSON" --out-dir "$OUTDIR"
code=$?
set -e
test "$code" -eq 1

echo "[test] mode=enforce invalid-contract -> 2"
set +e
bash scripts/ci/adr025-closure-release.sh --mode enforce --policy "$POLICY" --closure-json "$INVALID_JSON" --out-dir "$OUTDIR"
code=$?
set -e
test "$code" -eq 2

echo "[test] mode=warn invalid-contract -> 0"
bash scripts/ci/adr025-closure-release.sh --mode warn --policy "$POLICY" --closure-json "$INVALID_JSON" --out-dir "$OUTDIR"

echo "[test] done"
