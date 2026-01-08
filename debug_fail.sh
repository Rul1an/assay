#!/bin/bash
SERVER_BIN="./target/release/assay-mcp-server"
POLICY_ROOT="./examples/policies/targets"

check_debug() {
    local policy="$1"
    local tool="$2"
    local params="$3"

    ARGS_JSON=$(jq -n \
                --arg t "$tool" \
                --arg p "$policy" \
                --argjson a "$params" \
                '{tool: $t, policy: $p, arguments: $a}')

    REQ=$(jq -n \
          --argjson args "$ARGS_JSON" \
          '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"assay_check_args","arguments":$args}}')

    echo "Request: $REQ"
    echo "---"
    RESPONSE=$(echo "$REQ" | "$SERVER_BIN" --policy-root "$POLICY_ROOT" 2>/dev/null)
    echo "Response: $RESPONSE"
}

check_debug "filesystem-mcp-hardened.yaml" "read_file" "{\"path\":\"/workspace/readme.md\"}"
