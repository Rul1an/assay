# MCP Policy Files

This page documents the YAML schema consumed by `assay mcp wrap`.

Assay policy files decide:

- which MCP tools are allowed or denied
- how tool arguments are validated with JSON Schema
- which tools need extra controls such as approval, scope restriction, or argument redaction

## Supported Versions

- `version: "2.0"`: current format, with per-tool JSON Schema under `schemas:`
- `version: "1.0"`: legacy `constraints:` format

Assay still reads v1 policies, warns once, and can migrate them with `assay policy migrate`.

## Minimal v2 Policy

```yaml
version: "2.0"
name: "starter"

tools:
  allow: ["read_file", "list_dir"]
  deny: ["exec", "shell", "write_file"]

schemas:
  read_file:
    type: object
    additionalProperties: false
    properties:
      path:
        type: string
        pattern: "^/workspace/.*"
        minLength: 1
        maxLength: 4096
    required: ["path"]

  list_dir:
    type: object
    additionalProperties: false
    properties:
      path:
        type: string
        pattern: "^/workspace/.*"
        minLength: 1
        maxLength: 4096
    required: ["path"]

enforcement:
  unconstrained_tools: warn
```

## Top-Level Fields

| Field | Type | Meaning |
|------|------|---------|
| `version` | string | Policy schema version. Use `"2.0"` for new files. |
| `name` | string | Optional human-readable label. |
| `tools` | object | Allow/deny lists and obligation controls. |
| `allow` / `deny` | list<string> | Legacy aliases merged into `tools.allow` / `tools.deny` on load. |
| `schemas` | map<string, object> | JSON Schema per tool. `$defs` is reserved for shared definitions. |
| `constraints` | list<object> | Legacy v1 regex constraints. Deprecated. |
| `enforcement` | object | What to do with allowed tools that have no schema. |
| `limits` | object | Optional request and tool-call ceilings. |
| `signatures` | object | Optional tool-description integrity checks. |
| `tool_pins` | map<string, object> | Cryptographic pins for expected tool identity. |
| `discovery` | object | Advanced runtime discovery settings. |
| `runtime_monitor` | object | Advanced runtime monitoring rules. |
| `kill_switch` | object | Advanced kill-switch triggers. |

Unknown fields are ignored with a warning, so it is worth keeping this page and your checked-in policies aligned.

## `tools:` Fields

The `tools` section handles both filtering and extra controls.

| Field | Type | Meaning |
|------|------|---------|
| `allow` | list<string> | Allowed tool names or wildcard patterns. |
| `deny` | list<string> | Blocked tool names or wildcard patterns. |
| `allow_classes` | list<string> | Allow by tool taxonomy class. |
| `deny_classes` | list<string> | Deny by tool taxonomy class. |
| `approval_required` | list<string> | Tools that require a valid approval artifact. |
| `approval_required_classes` | list<string> | Approval requirement by tool class. |
| `restrict_scope` | list<string> | Tools whose arguments must match a scope contract. |
| `restrict_scope_classes` | list<string> | Scope restriction by tool class. |
| `restrict_scope_contract` | object | Shared contract used for `restrict_scope`. |
| `redact_args` | list<string> | Tools whose arguments should be redacted. |
| `redact_args_classes` | list<string> | Redaction by tool class. |
| `redact_args_contract` | object | Shared contract used for `redact_args`. |

### Wildcards

Assay uses simple `*` wildcards:

- `"*"` matches all tools
- `"read_*"` matches by prefix
- `"*_file"` matches by suffix
- `"*search*"` matches by substring
- patterns without `*` are exact matches

## JSON Schema in `schemas:`

Each tool can have a full JSON Schema for its argument object:

```yaml
schemas:
  create_ticket:
    type: object
    additionalProperties: false
    properties:
      title:
        type: string
        minLength: 5
        maxLength: 120
      priority:
        type: string
        enum: ["low", "medium", "high"]
      labels:
        type: array
        items:
          type: string
          maxLength: 32
    required: ["title", "priority"]
```

Recommended defaults for security-sensitive tools:

- `additionalProperties: false`
- `minLength: 1` on required strings
- explicit `required: [...]`
- bounded arrays and strings

If a call violates the schema, Assay denies it with `E_ARG_SCHEMA`.

## Shared Definitions With `$defs`

Assay supports local shared definitions via `$defs`.
Use `#/$defs/...` references inside tool schemas.

```yaml
schemas:
  $defs:
    safe_path:
      type: string
      pattern: "^/workspace/.*"
      minLength: 1
      maxLength: 4096

  read_file:
    type: object
    additionalProperties: false
    properties:
      path:
        $ref: "#/$defs/safe_path"
    required: ["path"]
```

## Enforcement

Allowed tools can still be considered unsafe if you do not attach a schema.
`enforcement.unconstrained_tools` decides what happens then:

```yaml
enforcement:
  unconstrained_tools: warn
```

Supported values:

- `warn`: allow the tool, but emit `E_TOOL_UNCONSTRAINED`
- `deny`: block allowed tools that have no schema
- `allow`: silently allow unconstrained tools

## Limits

```yaml
limits:
  max_requests_total: 1000
  max_tool_calls_total: 500
```

Exceeding these limits produces `E_RATE_LIMIT`.

## Approval, Scope Restriction, and Redaction

These controls sit next to ordinary allow/deny rules:

```yaml
tools:
  allow: ["read_file", "deploy_release", "create_ticket"]
  approval_required: ["deploy_release"]
  restrict_scope: ["read_file"]
  redact_args: ["create_ticket"]

  restrict_scope_contract:
    scope_type: "path_prefix"
    scope_value: "/workspace"
    scope_match_mode: "prefix"

  redact_args_contract:
    redaction_target: "args"
    redaction_mode: "mask"
    redaction_scope: "sensitive_fields"
```

Use these when you want:

- explicit human approval for risky tools
- runtime enforcement that file or resource arguments stay in-bounds
- redaction of secrets before downstream logging or evidence export

## Tool Pins

`tool_pins` protect against tool-definition drift by pinning the expected server, tool name, and hashes:

```yaml
tool_pins:
  read_file:
    server_id: "filesystem-prod"
    tool_name: "read_file"
    schema_hash: "9f4d4d0f..."
    meta_hash: "42f5df3e..."
```

## Legacy v1 Compatibility

Legacy v1 policies use `constraints:`:

```yaml
version: "1.0"
deny: ["exec"]
constraints:
  - tool: "read_file"
    params:
      path:
        matches: "^/workspace/.*"
```

Assay loads this shape, warns, normalizes `allow` / `deny` into `tools.*`, and auto-migrates constraints into in-memory JSON Schemas.

To write the v2 form to disk:

```bash
assay policy migrate --input policy.yaml
```

To fail CI if deprecated constructs are still present:

```bash
assay policy validate --deny-deprecations --input policy.yaml
```

## See Also

- [MCP Quick Start](../../mcp/quickstart.md)
- [OpenTelemetry & Langfuse](../../guides/otel-langfuse.md)
- [Migration Guide](../../migration/v1-to-v2.md)
