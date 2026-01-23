#!/bin/bash
set -e

# Configuration
ASSAY="./target/release/assay"
SERVER="tests/echo_server.py"
# Use absolute path for safety in run_command context
POLICIES="$(pwd)/tests/security_audit/policies"
RESULTS="$(pwd)/tests/security_audit/results.log"

echo "=== Starting Security Audit Baseline Tests ===" > "$RESULTS"

# Function to run a test with timeout
run_test() {
    local name="$1"
    local input="$2"
    local policy="$3"

    echo "--- Test: $name ---" >> "$RESULTS"
    echo "Input: $input" >> "$RESULTS"

    # Use gtimeout if available (macOS coreutils), else timeout
    local TIMEOUT_CMD="timeout"
    if command -v gtimeout &> /dev/null; then
        TIMEOUT_CMD="gtimeout"
    fi

    if [ -z "$policy" ]; then
        # No policy (Passthrough)
        # We enforce a 5 second timeout. If assay hangs, it gets killed.
        echo "$input" | $TIMEOUT_CMD 5s "$ASSAY" mcp wrap -- python3 "$SERVER" >> "$RESULTS" 2>&1 || echo "(TIMEOUT/ERROR)" >> "$RESULTS"
    else
        # With Policy
        echo "$input" | $TIMEOUT_CMD 5s "$ASSAY" mcp wrap --policy "$policy" --verbose -- python3 "$SERVER" >> "$RESULTS" 2>&1 || echo "(TIMEOUT/ERROR)" >> "$RESULTS"
    fi
    echo "" >> "$RESULTS"
}

# 1.1 Smoke Test
run_test "Smoke (Ping)" '{"jsonrpc": "2.0", "id": 1, "method": "ping"}' ""

# 1.2 Denylist
run_test "Denylist (delete_file)" \
    '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"delete_file","arguments":{"path":"/etc/passwd"}}}' \
    "$POLICIES/denylist.yaml"

# 1.3 Allowlist
run_test "Allowlist (write_file - implicit deny)" \
    '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"write_file","arguments":{}}}' \
    "$POLICIES/allowlist.yaml"

# 1.4 Constraints
run_test "Constraints (rm -rf)" \
    '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"run_command","arguments":{"command":"rm -rf /"}}}' \
    "$POLICIES/constraints.yaml"

# 1.5 Rate Limit (Burst)
echo "--- Test: Rate Limit ---" >> "$RESULTS"
(
    echo '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"read","arguments":{}}}'
    sleep 0.1
    echo '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"read","arguments":{}}}'
    sleep 0.1
    echo '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"read","arguments":{}}}'
    sleep 0.1
    echo '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"read","arguments":{}}}'
) | timeout 5s "$ASSAY" mcp wrap --policy "$POLICIES/ratelimit.yaml" -- python3 "$SERVER" >> "$RESULTS" 2>&1 || echo "(TIMEOUT/ERROR)" >> "$RESULTS"

echo "=== Completed ===" >> "$RESULTS"
