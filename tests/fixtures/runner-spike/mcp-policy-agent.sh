#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <work-dir>" >&2
  exit 64
fi

: "${ASSAY_BIN:?ASSAY_BIN must point to the assay CLI binary}"
: "${ASSAY_RUNNER_POLICY_DECISION_LOG:?ASSAY_RUNNER_POLICY_DECISION_LOG must be set}"
: "${ASSAY_RUNNER_RUN_ID:?ASSAY_RUNNER_RUN_ID must be set}"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
work_dir=$1
mkdir -p "$work_dir"

input_file="$work_dir/policy-input.txt"
policy_file="$work_dir/policy.yaml"
request_file="$work_dir/request.jsonl"
response_file="$work_dir/response.jsonl"

printf '%s\n' "assay runner policy fixture input" > "$input_file"
cat > "$policy_file" <<'YAML'
tools:
  allow:
    - read_file
YAML

python3 - "$input_file" > "$request_file" <<'PY'
import json
import sys

path = sys.argv[1]
print(
    json.dumps(
        {
            "jsonrpc": "2.0",
            "id": "runner-policy-request-001",
            "method": "tools/call",
            "params": {
                "name": "read_file",
                "arguments": {
                    "path": path,
                    "_meta": {"tool_call_id": "tc_runner_policy_001"},
                },
            },
        },
        separators=(",", ":"),
    )
)
PY

"$ASSAY_BIN" mcp wrap \
  --policy "$policy_file" \
  --decision-log "$ASSAY_RUNNER_POLICY_DECISION_LOG" \
  --event-source "assay://runner-spike/$ASSAY_RUNNER_RUN_ID" \
  --label runner-spike-policy-fixture \
  -- python3 "$ROOT/tests/fixtures/runner-spike/mcp_file_server.py" \
  < "$request_file" > "$response_file"

python3 - "$response_file" <<'PY'
import json
import sys

line = open(sys.argv[1], encoding="utf-8").readline()
response = json.loads(line)
content = response["result"]["content"][0]["text"]
if "assay runner policy fixture input" not in content:
    raise SystemExit("wrapped MCP response did not include fixture input")
PY
