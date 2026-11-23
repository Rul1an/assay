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
The answer is the one `assay mcp wrap` gives before it forwards the call. A call wrap refuses for
`approval_required`, `restrict_scope` or `redact_args` is `allowed: false` with `code`
`P_APPROVAL_REQUIRED`, `P_RESTRICT_SCOPE` or `P_REDACT_ARGS`. An approval travels in the call's
`arguments._meta.approval`, as it does through wrap.

When the policy file holds something this check does not weigh, the answer carries `notes`: a key
another tool or command reads (`sequences` for `assay_check_sequence`, `blocklist` for
`assay_policy_decide`, `discovery` for `assay discover`), a pin on this tool, which needs the
server's tool definition, and `limits`, which count the calls of a session. Wrap decides the last
two. A key no Assay reader knows is counted, not quoted; `assay policy validate` names it.

### `assay_check_sequence`
Validates sequence rules.
**Input**: `{ "history": ["tool1", ...], "next_tool": "string", "policy": "path.yaml" }`
**Output**: Same structure as `check_args`. The check reads a policy file's `sequences` and
`aliases`, and receives tool names only. `notes` names what it did not decide: a `tools.allow` or
`tools.deny` list (read by `assay_check_args`), `tools.require_args`, `tools.arg_constraints` or
`on_error` (read by neither check), and each rule that matches calls by their arguments, which
cannot match a name-only history. `version`, `name` and `metadata` are labels and get no note; any
other field makes the file `E_POLICY_PARSE`.

### `assay_policy_decide`
`assay_policy_decide` performs an exact-name check against its compatibility-only root `blocklist`.
It does not parse full `McpPolicy` controls such as `tools.allow` or `tools.deny`; use
`assay_check_args` for full, argument-aware policy evaluation. A root key other than `blocklist`
and the `version` and `name` labels is an error (`E_POLICY_PARSE`), not a clean allow. That
covers the canonical name-policy fields, every other policy section such as `schemas` or
`sequences`, and a misspelled `blocklist`.
**Input**: `{ "tool": "string", "policy": "path.yaml" }`
**Output**: `{ "allowed": boolean }` plus a short reason or match. Invalid roots and unsupported
dialects return `allowed: false` with `error.code: E_POLICY_PARSE`.

### Schemas used by the live wrapper

Before spawning the upstream process, `assay mcp wrap` indexes literal property
names and array item schemas for each configured tool. Named properties must
use ASCII without control characters and contain at least one letter or digit.
After removing punctuation and spaces and folding case, each name must remain distinct.
When an argument object contains a declared property it must use its exact
spelling. Objects with declared properties accept ASCII names for additional
members. A free-form nested map keeps its own key space.

The live profile supports literal `properties`, `required`, `additionalProperties`,
homogeneous `items`, and scalar constraints. It rejects constructs that would
require following references or choosing properties dynamically, including
`$ref`, `patternProperties`, draft switches, and conditional schemas, at wrapper
startup. For these policies, use a wrapper policy with explicit properties.
Policy evaluation continues to use the JSON Schema validator for the admitted
constraints. Live tool call argument strings and member names must be NUL-free.

### Live catalog observations

The live pin check uses a complete sequence of legacy `tools/list` pages.
A `nextCursor` string, including the empty string, signals another page.
Only a terminal page with the cursor omitted can complete the sequence.

A server using the 2026-07-28 list format sends `resultType`, `ttlMs` and
`cacheScope`. The live observer currently cannot renew a pin from that
format. This limits live enforcement even when the server response is valid.
