#!/bin/bash
echo "=== Test 3.4: Tool Poisoning ==="
echo '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"read_file","arguments":{}}}' | \
    ./target/release/assay mcp wrap --policy tests/security_audit/policies/allowlist.yaml --verbose -- python3 tests/security_audit/poisoned_server.py
