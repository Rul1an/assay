#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <work-dir>" >&2
  exit 64
fi

# The wrapper lives inside the fixture package since Phase 2D Slice 5B:
# `runner-fixtures/openai-agents/sdk-policy-agent.sh`. FIXTURE_DIR is
# the directory of this script. ROOT is two levels up
# (runner-fixtures/ -> repo root) and is used to locate the shared
# cross-runtime policy agent that has not yet moved to the fixtures
# package boundary (Slice 5C territory).
FIXTURE_DIR="${ASSAY_RUNNER_OPENAI_FIXTURE_DIR:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)}"
ROOT="${ASSAY_FIXTURE_ROOT:-$(cd "$FIXTURE_DIR/../.." && pwd)}"
FIXTURE_AGENT="${ASSAY_RUNNER_OPENAI_FIXTURE_AGENT:-$FIXTURE_DIR/fixture-agent.js}"
POLICY_AGENT="${ASSAY_RUNNER_POLICY_AGENT:-$ROOT/tests/fixtures/runner-spike/mcp-policy-agent.sh}"

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

export NODE_PATH="$FIXTURE_DIR/node_modules${NODE_PATH:+:$NODE_PATH}"
node "$FIXTURE_AGENT" "$1"
# The delegated full S5 gate captures the SDK fixture and policy fixture in one
# cgroup. Give the kernel-event reader a short phase boundary between the Node
# SDK burst and the policy subprocess so both fixture-input reads are captured
# without ring-buffer pressure. The bundle does not claim timing. Keep the
# default conservative because this path only runs in acceptance fixtures.
sleep "${ASSAY_RUNNER_PHASE_DRAIN_SLEEP:-1}"
"$POLICY_AGENT" "$1"
