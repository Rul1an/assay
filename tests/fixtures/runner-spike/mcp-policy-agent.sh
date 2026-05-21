#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <work-dir>" >&2
  exit 64
fi

: "${ASSAY_BIN:?ASSAY_BIN must point to the assay CLI binary}"
: "${ASSAY_RUNNER_POLICY_DECISION_LOG:?ASSAY_RUNNER_POLICY_DECISION_LOG must be set}"
: "${ASSAY_RUNNER_RUN_ID:?ASSAY_RUNNER_RUN_ID must be set}"

ROOT="${ASSAY_FIXTURE_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)}"
MCP_FILE_SERVER="${ASSAY_RUNNER_MCP_FILE_SERVER:-$ROOT/tests/fixtures/runner-spike/mcp_file_server.py}"
work_dir=$1
mkdir -p "$work_dir"

input_file="$work_dir/policy-input.txt"
if [ ! -f "$input_file" ]; then
  printf '%s\n' "assay runner policy fixture input" > "$input_file"
fi

if [ -d /dev/shm ]; then
  control_dir="$(mktemp -d /dev/shm/assay-runner-policy-control.XXXXXX)"
else
  control_dir="$(mktemp -d "${TMPDIR:-/tmp}/assay-runner-policy-control.XXXXXX")"
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

printf '{"jsonrpc":"2.0","id":"runner-policy-request-001","method":"tools/call","params":{"name":"read_file","arguments":{"path":"%s","_meta":{"tool_call_id":"tc_runner_policy_001"}}}}\n' \
  "$input_file" > "$request_file"

"$ASSAY_BIN" mcp wrap \
  --policy "$policy_file" \
  --decision-log "$ASSAY_RUNNER_POLICY_DECISION_LOG" \
  --event-source "assay://runner-spike/$ASSAY_RUNNER_RUN_ID" \
  --label runner-spike-policy-fixture \
  -- python3 "$MCP_FILE_SERVER" \
  < "$request_file" > "$response_file"

if ! grep -Fq "assay runner policy fixture input" "$response_file"; then
  echo "wrapped MCP response did not include fixture input" >&2
  exit 1
fi
