#!/bin/bash
set -e

# 1. Create a Policy with Rate Limits
# Max 2 tool calls total
cat > ratelimit_policy.yaml <<EOF
limits:
  max_tool_calls_total: 2
EOF

# 2. Mock Server
SERVER_SCRIPT="tests/echo_server.py"

echo "=== 1. Starting Rate Limit Test (Max 2 calls) ==="

# We pipe 3 requests.
# 1. Call 1 (Should Pass)
# 2. Call 2 (Should Pass)
# 3. Call 3 (Should Fail)

(
echo '{"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "read_file", "arguments": {"path": "1"}}}'
sleep 0.1
echo '{"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "read_file", "arguments": {"path": "2"}}}'
sleep 0.1
echo '{"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "read_file", "arguments": {"path": "3"}}}'
sleep 0.1
) | ./target/debug/assay mcp wrap --policy ratelimit_policy.yaml -- python3 "$SERVER_SCRIPT" > ratelimit_output.txt

cat ratelimit_output.txt

# Checks
if grep -q '"id": 1.*"result"' ratelimit_output.txt && grep -q '"id": 2.*"result"' ratelimit_output.txt; then
    echo "✅ First 2 calls passed."
else
    echo "❌ First 2 calls failed/missing."
    exit 1
fi

if grep -q "MCP_RATE_LIMIT" ratelimit_output.txt; then
    echo "✅ 3rd call blocked with MCP_RATE_LIMIT."
else
    echo "❌ 3rd call WAS NOT BLOCKED!"
    exit 1
fi
