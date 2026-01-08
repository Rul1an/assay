# Assay v1.5.1 Validated Test Plan

## Overview

This test plan validates the updated MCP Security Policies against the **Assay MCP Server (v1.5.1)**.

**Important Note on Compatibility:**
Assay v1.5.1 has two policy engines:
1.  **Core Engine** (`assay coverage`): Uses Regex-based `McpPolicy`.
2.  **Server Engine** (`assay-mcp-server`): Uses JSON Schema-based policies.

The policies in this directory (`examples/policies/targets/*.yaml`) are **JSON Schema** policies, designed for the **Server Engine**. They are **incompatible** with `assay coverage`.

## Prerequisites

1.  **Build the Server**:
    ```bash
    cargo build --release --bin assay-mcp-server
    ```

2.  **Policy Location**:
    Ensure policies are in `examples/policies/targets/`.

## Test Procedure

Since the server binary (`assay-mcp-server`) is a standalone policy evaluation server, we test it by sending `assay_check_args` tool calls via JSON-RPC.

### 1. Automated Validation Script

Use the following script to verify all policies:

```bash
#!/bin/bash
# run_tests.sh
SERVER_BIN="./target/release/assay-mcp-server"
POLICY_ROOT="./examples/policies/targets"

# Function to verify a policy decision
# Usage: check_policy POLICY_FILE TOOL_NAME ARGS_JSON EXPECTED_STATUS
check_policy() {
    local policy="$1"
    local tool="$2"
    local args="$3"
    local expected="$4" # "true" (Allow) or "false" (Deny)

    # 1. Construct the assay_check_args call
    # We nest the target tool/args inside the assay_check_args arguments
    REQ_JSON=$(jq -n \
        --arg t "$tool" \
        --arg p "$policy" \
        --argjson a "$args" \
        '{jsonrpc: "2.0", id: 1, method: "tools/call", params: {name: "assay_check_args", arguments: {tool: $t, policy: $p, arguments: $a}}}')

    # 2. Send to server
    RESPONSE=$(echo "$REQ_JSON" | "$SERVER_BIN" --policy-root "$POLICY_ROOT" 2>/dev/null)

    # 3. Parse result
    # Result is located in result.content[0].text (stringified JSON)
    INNER_JSON=$(echo "$RESPONSE" | jq -r '.result.content[0].text // empty')

    if [ -z "$INNER_JSON" ]; then
        echo "❌ $policy: $tool -> CRASH/EMPTY RESPONSE"
        return 1
    fi

    ALLOWED=$(echo "$INNER_JSON" | jq -r '.allowed')

    if [ "$ALLOWED" == "$expected" ]; then
        STATUS_ICON="✅"
    else
        STATUS_ICON="❌"
    fi

    EXPECTED_LABEL="DENY"
    if [ "$expected" == "true" ]; then EXPECTED_LABEL="ALLOW"; fi

    ACTUAL_LABEL="DENY"
    if [ "$ALLOWED" == "true" ]; then ACTUAL_LABEL="ALLOW"; fi

    echo "$STATUS_ICON $policy: $tool -> $ACTUAL_LABEL (Expected $EXPECTED_LABEL)"

    if [ "$ALLOWED" != "$expected" ]; then
        echo "   Reason: $(echo "$INNER_JSON" | jq -c '.violations // .error')"
    fi
}

echo "=== Filesystem Hardened Tests ==="
check_policy "filesystem-mcp-hardened.yaml" "read_file" '{"path":"/workspace/readme.md"}' "true"
check_policy "filesystem-mcp-hardened.yaml" "read_file" '{"path":"/etc/passwd"}' "false"
check_policy "filesystem-mcp-hardened.yaml" "create_symlink" '{"path":"/a"}' "false"

echo "=== Figma Safe Tests ==="
check_policy "figma-mcp-safe.yaml" "get_figma_data" '{"fileKey":"ABC123xyz789"}' "true"
check_policy "figma-mcp-safe.yaml" "get_figma_data" '{"fileKey":"$(id)"}' "false"

echo "=== Playwright Hardened Tests ==="
check_policy "playwright-mcp-hardened.yaml" "browser_navigate" '{"url":"https://github.com"}' "true"
check_policy "playwright-mcp-hardened.yaml" "browser_navigate" '{"url":"http://localhost:8080"}' "false"
check_policy "playwright-mcp-hardened.yaml" "browser_execute_javascript" '{}' "false"
```

### 2. Manual Verification (via MCP Inspector)

You can also use the MCP Inspector to interactively test the policy server:

```bash
# Terminal 1: Start Server
./target/release/assay-mcp-server --policy-root examples/policies/targets

# Terminal 2: Connect Inspector
npx @modelcontextprotocol/inspector localhost:3000
```

In the inspector:
1.  Select tool: `assay_check_args`
2.  Arguments:
    ```json
    {
      "tool": "read_file",
      "policy": "filesystem-mcp-hardened.yaml",
      "arguments": {
        "path": "/etc/passwd"
      }
    }
    ```
3.  Execute and verify result matches `{ "allowed": false, ... }`

## Test Matrix results (v1.5.1)

| Policy | Test | Input | Result |
|--------|------|-------|--------|
| **FS-Hardened** | Safe Read | `/workspace/readme.md` | ✅ ALLOW |
| **FS-Hardened** | Root Read | `/etc/passwd` | ✅ DENY (Path mismatch) |
| **FS-Hardened** | Symlink | `create_symlink` | ✅ DENY (Missing tool) |
| **Figma-Safe** | Valid Key | `ABC123xyz...` | ✅ ALLOW |
| **Figma-Safe** | Injection | `$(id)` | ✅ DENY (Pattern mismatch) |
| **Playwright-H** | HTTPS | `https://github.com` | ✅ ALLOW |
| **Playwright-H** | Localhost | `http://localhost` | ✅ DENY (Pattern mismatch) |
