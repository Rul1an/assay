#!/bin/bash
SERVER_BIN="./target/release/assay-mcp-server"
POLICY_ROOT="./examples/policies/targets"

# Function to check policy decision
check_decision() {
    local policy="$1" # e.g. filesystem-mcp-hardened.yaml
    local tool="$2"
    local params="$3" # JSON string of tool arguments
    local expected="$4" # ALLOW or DENY

    echo -n "Checking $tool in $policy... "

    # Tool: assay_check_args
    # Arguments: { "tool": "read_file", "arguments": {...}, "policy": "..." }
    # Or assay_policy_decide?
    # assay_policy_decide takes { "tool": "name", "policy": "..." } -> This just checks allow/deny list?
    # No, check_args validates schema constraints.

    # We want to check if args are allowed.
    # Let's use `assay_check_args` which seems most relevant for "is this call allowed?"

    # Construct arguments for assay_check_args
    ARGS_JSON=$(jq -n \
                --arg t "$tool" \
                --arg p "$policy" \
                --argjson a "$params" \
                '{tool: $t, policy: $p, arguments: $a}')

    REQ=$(jq -n \
          --argjson args "$ARGS_JSON" \
          '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"assay_check_args","arguments":$args}}')

    RESPONSE=$(echo "$REQ" | "$SERVER_BIN" --policy-root "$POLICY_ROOT" 2>/dev/null)

    # Extract "valid" field from result
    # Result format: { content: [ { text: "JSON" } ] }
    # Inner JSON: { "valid": true/false, "errors": [...] }

    INNER_JSON=$(echo "$RESPONSE" | jq -r '.result.content[0].text')
    VALID=$(echo "$INNER_JSON" | jq -r '.valid')

    if [ "$VALID" == "true" ]; then
        ACTUAL="ALLOW"
    else
        ACTUAL="DENY"
    fi

    if [ "$ACTUAL" == "$expected" ]; then
        echo "✅ PASS ($ACTUAL)"
    else
        echo "❌ FAIL (Expected $expected, got $ACTUAL)"
        # Print error reason if denied unexpectedly
        if [ "$ACTUAL" == "DENY" ]; then
             echo "$INNER_JSON" | jq .
        fi
    fi
}

echo "=== Filesystem Hardened ==="
check_decision "filesystem-mcp-hardened.yaml" "read_file" "{\"path\":\"/workspace/readme.md\"}" "ALLOW"
check_decision "filesystem-mcp-hardened.yaml" "read_file" "{\"path\":\"/etc/passwd\"}" "DENY"
check_decision "filesystem-mcp-hardened.yaml" "create_symlink" "{\"path\":\"/a\"}" "DENY"

echo "=== Figma Safe ==="
check_decision "figma-mcp-safe.yaml" "get_figma_data" "{\"fileKey\":\"ABC123xyz789\"}" "ALLOW"
check_decision "figma-mcp-safe.yaml" "get_figma_data" "{\"fileKey\":\"\$(id)\"}" "DENY"

echo "=== Playwright Hardened ==="
check_decision "playwright-mcp-hardened.yaml" "browser_navigate" "{\"url\":\"https://google.com\"}" "ALLOW"
check_decision "playwright-mcp-hardened.yaml" "browser_navigate" "{\"url\":\"file:///etc/passwd\"}" "DENY"
