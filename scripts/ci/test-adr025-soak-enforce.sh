#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

POLICY="schemas/soak_readiness_policy_v1.json"

echo "[test] pass case"
bash scripts/ci/adr025-soak-enforce.sh --policy "$POLICY" --readiness scripts/ci/fixtures/adr025/readiness_pass.json
echo "ok"

echo "[test] policy fail case"
set +e
bash scripts/ci/adr025-soak-enforce.sh --policy "$POLICY" --readiness scripts/ci/fixtures/adr025/readiness_policy_fail.json
code=$?
set -e
test "$code" -eq 1

echo "[test] measurement/contract fail case"
set +e
bash scripts/ci/adr025-soak-enforce.sh --policy "$POLICY" --readiness scripts/ci/fixtures/adr025/readiness_contract_fail.json
code=$?
set -e
test "$code" -eq 2

echo "[test] done"
