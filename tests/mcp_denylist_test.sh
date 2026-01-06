#!/bin/bash
set -e

# 1. Create a Policy that Denies 'delete_file'
cat > deny_test_policy.yaml <<EOF
tools:
  deny:
    - delete_file
EOF

# 2. Mock Server that just echoes
SERVER_SCRIPT="tests/echo_server.py"

echo "=== 1. Testing Allowed Tool (read_file) ==="
response_allow=$(echo '{"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "read_file", "arguments": {"path": "foo"}}}' | ./target/debug/assay mcp wrap --policy deny_test_policy.yaml -- python3 "$SERVER_SCRIPT")
echo "$response_allow"

if echo "$response_allow" | grep -q "Called tool"; then
    echo "✅ Allowed tool passed."
else
    echo "❌ Allowed tool failed."
    exit 1
fi

echo "=== 2. Testing Denied Tool (delete_file) ==="
response_deny=$(echo '{"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "delete_file", "arguments": {"path": "foo"}}}' | ./target/debug/assay mcp wrap --policy deny_test_policy.yaml -- python3 "$SERVER_SCRIPT")
echo "$response_deny"

if echo "$response_deny" | grep -q "MCP_TOOL_DENIED"; then
    echo "✅ Denied tool blocked correctly."
else
    echo "❌ Denied tool WAS NOT BLOCKED!"
    exit 1
fi
