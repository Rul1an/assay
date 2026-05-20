#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
if ! node -e 'process.exit(Number(process.versions.node.split(".")[0]) >= 22 ? 0 : 1)'; then
  echo "SKIP: runner-spike OpenAI Agents fixture requires Node.js 22 or newer." >&2
  exit 40
fi

export ASSAY_RUNNER_ACCEPTANCE_AGENT_SCRIPT="$ROOT/tests/fixtures/runner-spike/openai-agents-sdk-policy-agent.sh"
export ASSAY_RUNNER_ACCEPTANCE_RUN_ID="${ASSAY_RUNNER_ACCEPTANCE_RUN_ID:-run_openai_agents_sdk_policy_correlation}"
export ASSAY_RUNNER_ACCEPTANCE_EXPECT_SDK_SOURCE="openai-agents-js-fixture"
export ASSAY_RUNNER_ACCEPTANCE_EXPECT_SDK_VERSION="0.11.4"

exec "$ROOT/scripts/ci/runner-spike-sdk-policy-correlation.sh"
