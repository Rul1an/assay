#!/bin/bash
set -e

# Compile latest
cargo build -p assay-cli --quiet

# Echo Server Path
SERVER_SCRIPT="tests/echo_server.py"

echo "=== 1. Starting MCP Proxy with Echo Server ==="
echo "Wrapped Command: python3 $SERVER_SCRIPT"

# We pipe input into assay mcp wrap
# The proxy forwards it to python echo server
# The server responds 'pong'
# The proxy forwards it back to us

response=$(echo '{"jsonrpc": "2.0", "id": 1, "method": "ping"}' | ./target/debug/assay mcp wrap -- python3 "$SERVER_SCRIPT")

echo "=== 2. Inspecting Response ==="
echo "$response"

if echo "$response" | grep -q "pong"; then
    echo "✅ Success: Passthrough verified."
else
    echo "❌ Failure: Did not receive pong."
    exit 1
fi
