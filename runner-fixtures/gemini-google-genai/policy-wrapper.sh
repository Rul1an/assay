#!/usr/bin/env bash
# Gemini-specific MCP policy fixture wrapper.
#
# Mirrors tests/fixtures/runner-spike/mcp-policy-agent.sh structurally, but the
# tool_call_id is read from the environment instead of being hardcoded to
# `tc_runner_policy_001`. This is the key isolation point: the Gemini fixture's
# stable tool_call_id comes from the cassette's recorded FunctionCall.id, and
# the policy fixture MUST bind to the same value for SDK <-> policy correlation
# under the level-3 stable-identity rule.
#
# Per #1307 "The implementation PR may share the existing MCP file server used
# by the S5 fixture, or provide a Gemini-specific wrapper if scope reasons
# require." This is the wrapper variant. The S5 mcp-policy-agent.sh is
# intentionally NOT modified by this PR.
set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <work-dir>" >&2
  exit 64
fi

: "${ASSAY_BIN:?ASSAY_BIN must point to the assay CLI binary}"
: "${ASSAY_RUNNER_POLICY_DECISION_LOG:?ASSAY_RUNNER_POLICY_DECISION_LOG must be set}"
: "${ASSAY_RUNNER_RUN_ID:?ASSAY_RUNNER_RUN_ID must be set}"
: "${ASSAY_RUNNER_SDK_TOOL_CALL_ID:?ASSAY_RUNNER_SDK_TOOL_CALL_ID must be set (Gemini fixture binds policy id to SDK id from cassette)}"

ROOT="${ASSAY_FIXTURE_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
MCP_FILE_SERVER="${ASSAY_RUNNER_MCP_FILE_SERVER:-$ROOT/tests/fixtures/runner-spike/mcp_file_server.py}"
work_dir=$1
mkdir -p "$work_dir"

input_file="$work_dir/policy-input.txt"
if [ ! -f "$input_file" ]; then
  printf '%s\n' "assay runner policy fixture input" > "$input_file"
fi

if [ -d /dev/shm ]; then
  control_dir="$(mktemp -d /dev/shm/assay-runner-gemini-policy-control.XXXXXX)"
else
  control_dir="$(mktemp -d "${TMPDIR:-/tmp}/assay-runner-gemini-policy-control.XXXXXX")"
fi
cleanup() {
  rm -rf -- "$control_dir"
}
trap cleanup EXIT

policy_file="$control_dir/policy.yaml"
request_file="$control_dir/request.jsonl"
response_file="$control_dir/response.jsonl"

cat > "$policy_file" <<'YAML'
tools:
  allow:
    - read_file
YAML

# Use the cassette-derived tool_call_id (passed in via env). This is the
# critical contract: the policy event's tool_call_id MUST equal the SDK
# event's tool_call_id which MUST equal FunctionCall.id from the recorded
# Gemini response. If they differ, the correlation report's binding will
# fail.
printf '{"jsonrpc":"2.0","id":"runner-policy-request-001","method":"tools/call","params":{"name":"read_file","arguments":{"path":"%s","_meta":{"tool_call_id":"%s"}}}}\n' \
  "$input_file" "$ASSAY_RUNNER_SDK_TOOL_CALL_ID" > "$request_file"

"$ASSAY_BIN" mcp wrap \
  --policy "$policy_file" \
  --decision-log "$ASSAY_RUNNER_POLICY_DECISION_LOG" \
  --event-source "assay://runner-spike/$ASSAY_RUNNER_RUN_ID" \
  --label runner-spike-gemini-policy-fixture \
  -- python3 "$MCP_FILE_SERVER" \
  < "$request_file" > "$response_file"

if ! grep -Fq "assay runner policy fixture input" "$response_file"; then
  echo "wrapped MCP response did not include fixture input" >&2
  exit 1
fi
