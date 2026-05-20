#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
export ASSAY_RUNNER_ACCEPTANCE_CORRELATION_SCRIPT="$ROOT/scripts/ci/runner-spike-openai-agents-sdk-policy-correlation.sh"
export ASSAY_RUNNER_ACCEPTANCE_LABEL="OpenAI Agents SDK+policy"
export ASSAY_RUNNER_ACCEPTANCE_RUN_ID="${ASSAY_RUNNER_ACCEPTANCE_RUN_ID:-run_openai_agents_sdk_policy_determinism}"

exec "$ROOT/scripts/ci/runner-spike-sdk-policy-three-run-determinism.sh"
