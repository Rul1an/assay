# Assay MCP API Reference

The Assay MCP Server exposes tools for agent self-verification.

## Error Handling
All tools return a standardized error structure if the operation cannot be performed (e.g., policy missing).
Note: This is an **Application-Level Error**, returned within the JSON-RPC `result`.
Stdio invalid JSON lines are ignored and produce no JSON-RPC response; the
server continues to the next line. On a valid `tools/call`, missing or malformed tool arguments
are not a distinct client error: they are published as the
fixed outer-dispatch payload `E_INTERNAL` / `Tool execution failed` with
`allowed: false` and `isError: true`. Unknown JSON-RPC methods remain protocol
error `-32601`.

### Error Shape

Tool results use the MCP `CallToolResult` envelope. The first text content item
contains the Assay payload:

```json
{
  "result": {
    "content": [{
      "type": "text",
      "text": "{\"allowed\":false,\"error\":{\"code\":\"E_CODE_STRING\",\"message\":\"Bounded message\"}}"
    }],
    "isError": true
  }
}
```

Outer dispatch failures use fixed, value-free messages. `E_INTERNAL` means
`Tool execution failed`; `E_TIMEOUT` means `Tool execution timed out`. Both
have `allowed: false` and `isError: true`. Caller `arguments.on_error` cannot
change that behavior. Unknown JSON-RPC methods use protocol error `-32601`.

### Common Error Codes
| Code | Description |
|---|---|
| `E_POLICY_NOT_FOUND` | The specified policy file does not exist. |
| `E_POLICY_READ` | Failed to read the policy file (permissions, etc.). |
| `E_PERMISSION_DENIED` | Access denied (e.g., policy path is outside the allowed root). |
| `E_INTERNAL` | The outer tool dispatch failed; raw internal error details are not emitted. |
| `E_TIMEOUT` | The outer tool dispatch exceeded the configured timeout. |

## Tools

### `assay_check_args`
Validates tool arguments against a schema.
**Input**: `{ "tool": "string", "arguments": {}, "policy": "path/to/policy.yaml" }`
**Output**:
```json
{
  "allowed": boolean,
  "violations": [{ "constraint": "...", "suggestion": "..." }],
  "suggested_fix": { ... } | null
}
```

### `assay_check_sequence`
Validates sequence rules.
**Input**: `{ "history": ["tool1", ...], "next_tool": "string", "policy": "path.yaml" }`
**Output**: Same structure as `check_args`.

### `assay_policy_decide`
`assay_policy_decide` performs an exact-name check against its compatibility-only root `blocklist`.
It does not parse full `McpPolicy` controls such as `tools.allow` or `tools.deny`; use
`assay_check_args` for full, argument-aware policy evaluation. Passing canonical name-policy fields
to `assay_policy_decide` is an error, not a clean allow.
**Input**: `{ "tool": "string", "policy": "path.yaml" }`
**Output**: `{ "allowed": boolean }` plus a short reason or match. Invalid roots and unsupported
dialects return `allowed: false` with `error.code: E_POLICY_PARSE`.
