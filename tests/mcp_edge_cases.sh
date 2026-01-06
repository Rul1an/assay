#!/bin/bash
set -e

SERVER_SCRIPT="tests/echo_server.py"
POLICY="edge_case_policy.yaml"
AUDIT_LOG="edge_audit.jsonl"
rm -f "$AUDIT_LOG"

# 1. Create Allowlist Policy (Only 'allowed_tool' is allowed)
cat > "$POLICY" <<EOF
tools:
  allow:
    - allowed_tool
EOF

echo "=== 1. Testing Allowlist (Implicit Deny) ==="
# 'allowed_tool' should pass
output_allow=$(echo '{"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "allowed_tool", "arguments": {}}}' | \
    ./target/debug/assay mcp wrap --policy "$POLICY" --verbose -- python3 "$SERVER_SCRIPT" 2>&1)

if echo "$output_allow" | grep -q "ALLOW allowed_tool"; then
    echo "✅ Allowed tool passed."
else
    echo "❌ Allowed tool failed."
    echo "$output_allow"
    exit 1
fi

# 'other_tool' should fail
output_deny=$(echo '{"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "other_tool", "arguments": {}}}' | \
    ./target/debug/assay mcp wrap --policy "$POLICY" --verbose -- python3 "$SERVER_SCRIPT" 2>&1)

if echo "$output_deny" | grep -q "DENY other_tool"; then
    echo "✅ Unlisted tool denied (Implicit Block)."
else
    echo "❌ Unlisted tool NOT denied."
    echo "$output_deny"
    exit 1
fi

echo "=== 2. Testing Passthrough (Non-tool Requests) ==="
# resources/list should pass implicitly (policy only checks tools/call)
output_resource=$(echo '{"jsonrpc": "2.0", "id": 3, "method": "resources/list", "params": {}}' | \
    ./target/debug/assay mcp wrap --policy "$POLICY" --verbose -- python3 "$SERVER_SCRIPT" 2>&1)

# It won't log ALLOW because we only log specific tool allow decisions?
# Actually code says: if req.is_tool_call() ...
# So we expect NO "DENY" and server response.
if echo "$output_resource" | grep -q "DENY"; then
    echo "❌ Non-tool request was DENIED."
    exit 1
fi
# The echo server echoes input.
if echo "$output_resource" | grep -q "resources/list"; then
    echo "✅ Non-tool request passed through."
else
    echo "❌ Non-tool request did not echo back."
    echo "$output_resource"
    exit 1
fi

echo "=== 3. Testing Malformed JSON (Passthrough) ==="
# Sending garbage. Proxy fails parse -> forwards -> server ignores/errors?
# Echo server prints "Received: ..." if it gets it.
output_garbage=$(echo '{ "jsonrpc": "broken...' | \
    ./target/debug/assay mcp wrap --policy "$POLICY" --verbose -- python3 "$SERVER_SCRIPT" 2>&1)

if echo "$output_garbage" | grep -q "jsonrpc"; then
     echo "✅ Malformed JSON passed through (Server received it)."
else
     echo "❌ Malformed JSON blocked or lost."
     exit 1
fi

echo "🎉 Edge Case Tests Passed!"
