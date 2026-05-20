#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <work-dir>" >&2
  exit 64
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
FIXTURE_DIR="$ROOT/tests/fixtures/runner-spike/openai-agents-js"

: "${ASSAY_RUNNER_SDK_TOOL_CALL_ID:=tc_runner_policy_001}"
export ASSAY_RUNNER_SDK_TOOL_CALL_ID
export OPENAI_AGENTS_DISABLE_TRACING=1

if ! command -v node >/dev/null 2>&1; then
  echo "error: node is required to run $FIXTURE_DIR/fixture-agent.js but was not found on PATH" >&2
  exit 69
fi

if [ ! -d "$FIXTURE_DIR/node_modules/@openai/agents" ]; then
  echo "error: missing fixture dependency '@openai/agents' under $FIXTURE_DIR/node_modules" >&2
  echo "hint: run 'npm ci' in $FIXTURE_DIR before running this script" >&2
  exit 69
fi

node "$FIXTURE_DIR/fixture-agent.js" "$1"
"$ROOT/tests/fixtures/runner-spike/mcp-policy-agent.sh" "$1"
