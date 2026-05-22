#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
if ! command -v node >/dev/null 2>&1; then
  echo "SKIP: runner-spike OpenAI Agents fixture requires Node.js 22 or newer; node was not found on PATH." >&2
  exit 40
fi

if ! node -e 'process.exit(Number(process.versions.node.split(".")[0]) >= 22 ? 0 : 1)'; then
  echo "SKIP: runner-spike OpenAI Agents fixture requires Node.js 22 or newer." >&2
  exit 40
fi

export ASSAY_RUNNER_ACCEPTANCE_AGENT_SCRIPT="$ROOT/runner-fixtures/openai-agents/sdk-policy-agent.sh"
export ASSAY_RUNNER_ACCEPTANCE_RUN_ID="${ASSAY_RUNNER_ACCEPTANCE_RUN_ID:-run_openai_agents_sdk_policy_correlation}"
# Phase 2D Slice 5B renamed the source identity from 'openai-agents-js-fixture'
# to 'openai-agents-fixture' to align with the new runner-fixtures/openai-agents/
# package boundary (fixture identity, not language identity).
export ASSAY_RUNNER_ACCEPTANCE_EXPECT_SDK_SOURCE="openai-agents-fixture"
export ASSAY_RUNNER_ACCEPTANCE_EXPECT_SDK_VERSION="0.11.4"

exec "$ROOT/scripts/ci/runner-spike-sdk-policy-correlation.sh"
