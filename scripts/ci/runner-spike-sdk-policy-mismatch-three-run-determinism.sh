#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

export ASSAY_RUNNER_ACCEPTANCE_CORRELATION_SCRIPT="$ROOT/scripts/ci/runner-spike-sdk-policy-mismatch.sh"
export ASSAY_RUNNER_ACCEPTANCE_LABEL="SDK+policy mismatch"
export ASSAY_RUNNER_ACCEPTANCE_RUN_ID="${ASSAY_RUNNER_ACCEPTANCE_RUN_ID:-run_sdk_policy_mismatch_determinism}"

exec "$ROOT/scripts/ci/runner-spike-sdk-policy-three-run-determinism.sh"
