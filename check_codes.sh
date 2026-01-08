#!/bin/bash
SERVER_BIN="./target/release/assay-mcp-server"
POLICY_ROOT="./examples/policies/targets"

# Helper to run a test and print the error code
check_code() {
    local policy="$1"
    local tool="$2"
    local args="$3"
    local desc="$4"

    REQ="{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"$tool\",\"arguments\":$args}}"
    MODIFIED_REQ=$(echo "$REQ" | jq -c ".params.arguments += {\"policy\": \"$policy.yaml\"}")

    RESPONSE=$(echo "$MODIFIED_REQ" | "$SERVER_BIN" --policy-root "$POLICY_ROOT" 2>/dev/null)

    # Extract error code if present
    CODE=$(echo "$RESPONSE" | jq -r '.result.content[0].text | fromjson | .error_code // "OK"')
    echo "Test [$desc]: $CODE"
}

echo "=== Filesystem Hardened ==="
check_code "filesystem-mcp-hardened" "read_file" "{\"path\":\"/etc/passwd\"}" "FS-H-02 (Block sensitive file)"
check_code "filesystem-mcp-hardened" "create_symlink" "{\"path\":\"/a\",\"target\":\"/b\"}" "FS-H-05 (Block missing tool)"

echo "=== Figma Safe ==="
check_code "figma-mcp-safe" "get_figma_data" "{\"fileKey\":\"\$(id)\"}" "FIG-02 (Block injection)"
check_code "figma-mcp-safe" "download_figma_images" "{}" "FIG-07 (Block missing tool)"

echo "=== Playwright Hardened ==="
check_code "playwright-mcp-hardened" "browser_navigate" "{\"url\":\"file:///etc/passwd\"}" "PW-H-05 (Block file://)"
check_code "playwright-mcp-hardened" "browser_execute_javascript" "{}" "PW-H-07 (Block missing tool)"
