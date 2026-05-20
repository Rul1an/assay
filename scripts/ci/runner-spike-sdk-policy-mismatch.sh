#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

ASSAY_RUNNER_ACCEPTANCE_SDK_TOOL_CALL_ID="${ASSAY_RUNNER_ACCEPTANCE_SDK_TOOL_CALL_ID:-tc_runner_sdk_only_001}" \
  ASSAY_RUNNER_ACCEPTANCE_EXPECT_SDK_POLICY_MISMATCH=1 \
  "$ROOT/scripts/ci/runner-spike-sdk-policy-correlation.sh"
