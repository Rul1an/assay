# Assay MCP API Reference (v0.5.0)

The Assay MCP Server exposes tools for agent self-verification.

## Error Handling
All tools return a standardized error structure if the operation cannot be performed (e.g., policy missing).
Note: This is an **Application-Level Error**, returned within the JSON-RPC `result`. Protocol-level errors (invalid JSON) return a JSON-RPC `error`.

### Error Shape
```json
{
  "result": {
    "error": {
      "code": "E_CODE_STRING",
      "message": "Human readable message",
      "details": { ... } // Optional
    }
  }
}
```

### Common Error Codes
| Code | Description |
|---|---|
| `E_POLICY_NOT_FOUND` | The specified policy file does not exist. |
| `E_POLICY_READ` | Failed to read the policy file (permissions, etc.). |
| `E_PERMISSION_DENIED` | Access denied (e.g., policy path is outside the allowed root). |

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
Checks blocklists.
**Input**: `{ "tool": "string", "policy": "path.yaml" }`
**Output**: Same structure as `check_args` (allowed/denied).
