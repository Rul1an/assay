# Runtime enforcement (Proxy + Server)

Assay enforces MCP policies in two runtime paths:

- **Proxy**: `assay mcp wrap --policy <POLICY> -- <SERVER_CMD...>`
- **Server**: `assay-mcp-server --policy-root <DIR>`

Both paths call the **same core evaluation API**:
`assay_core::mcp::policy::McpPolicy::evaluate(...)`

This guarantees that:
- `assay coverage` / `assay validate` logic matches runtime enforcement
- one policy file works consistently in CI and production

---

## Decision outcomes

The engine returns one of:

- `Allow`
- `AllowWithWarning` (typically `E_TOOL_UNCONSTRAINED` in warn mode)
- `Deny` with a structured contract (includes `error_code`)

---

## Deny response structure

When a tool call is denied, the response includes a structured contract.
Depending on MCP client expectations, this may appear as either:

- `structuredContent` (camelCase), or
- `structured_content` (snake_case)

Consumers should accept both during the transition period.

The contract contains:

- `status: "deny"`
- `error_code: <E_...>`
- `tool: <tool name>`
- optional details such as schema violations

Example (simplified):

```json
{
  "status": "deny",
  "error_code": "E_ARG_SCHEMA",
  "tool": "read_file",
  "violations": [
    { "path": "/path", "message": "..." }
  ]
}
```

---

## Error code mapping

Runtime uses the same canonical codes as CLI:
- `E_TOOL_DENIED`
- `E_TOOL_NOT_ALLOWED`
- `E_ARG_SCHEMA`
- `E_TOOL_UNCONSTRAINED`
- `E_RATE_LIMIT`
- `E_POLICY_INVALID`

---

## Operational guidance

- Use `enforcement.unconstrained_tools: warn` in development to catch missing schemas without breaking flows.
- Use `deny` in production hardened environments.
- Prefer `additionalProperties: false` in schemas to prevent hidden/extra arguments.
- Keep `$ref` scoped to `#/` only (no remote refs).
