#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <work-dir>" >&2
  exit 64
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
work_dir=$1

ASSAY_RUNNER_SDK_TOOL_CALL_ID=tc_runner_policy_001 \
  "$ROOT/tests/fixtures/runner-spike/sdk-event-wrapper.sh" "$work_dir"

"$ROOT/tests/fixtures/runner-spike/mcp-policy-agent.sh" "$work_dir"
