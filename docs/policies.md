# Policies (v2.0)

Assay policies define which MCP tools are allowed and how tool arguments are validated.
Since Assay v1.6.0, **argument constraints are expressed as JSON Schema** (policy schema version `"2.0"`).

This document is the **source of truth** for policy syntax and semantics.

---

## Policy schema versions

- **Policy `version: "2.0"`**: JSON Schema constraints via `schemas:`.
- **Policy `version: "1.0"`**: legacy regex constraints via `constraints:` (deprecated).
  - v1 policies are **auto-migrated in memory** (with a warning).
  - Use `assay policy migrate` to write v2 output.

---

## Minimal v2.0 policy

```yaml
version: "2.0"
name: "starter"

tools:
  allow: ["read_file"]

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

enforcement:
  unconstrained_tools: warn
```

---

## Tool filtering

Tool filtering is independent of argument schemas.

```yaml
tools:
  allow: ["read_file", "list_directory", "search_*"]
  deny:  ["execute_*", "spawn", "*sh", "*kill*"]
```

### Wildcards

Assay supports simple `*` wildcards:
- `"*"` matches all tools.
- `"exec*"` prefix match (starts_with)
- `"*sh"` suffix match (ends_with)
- `"*kill*"` contains match
- patterns without `*` are exact match.

> Note: this is not full globbing; it is intentionally minimal and predictable.

---

## Argument schemas (JSON Schema)

Schemas are provided per tool under `schemas:`.

```yaml
schemas:
  read_file:
    type: object
    additionalProperties: false
    properties:
      path:
        type: string
        pattern: "^/workspace/.*"
    required: ["path"]
```

### Security defaults (recommended)
- `additionalProperties: false` (prevents hidden/extra args)
- string bounds: `minLength: 1`, `maxLength: 4096`
- require sensitive parameters (`required: [...]`)

Assay will deny tool calls that violate schema constraints with `E_ARG_SCHEMA`.

---

## `$defs` / `$ref` support (scoped)

Assay supports `$ref` only inside the same policy document (e.g. `#/schemas/$defs/...`).
No remote refs are allowed.

Example:

```yaml
version: "2.0"
name: "with-defs"

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
      path: { $ref: "#/schemas/$defs/safe_path" }
    required: ["path"]
```

---

## Enforcement modes

Tools can be allowed but unconstrained (no schema). Enforcement controls what happens then:

```yaml
enforcement:
  unconstrained_tools: warn   # warn | deny | allow
```

- `warn` (default): allow the call, but emit `E_TOOL_UNCONSTRAINED` as a warning.
- `deny`: block unconstrained tool calls.
- `allow`: silently allow unconstrained tool calls (legacy feel; not recommended for prod).

---

## Limits

```yaml
limits:
  max_requests_total: 1000
  max_tool_calls_total: 500
```

Exceeding limits produces `E_RATE_LIMIT`.

---

## Signatures

```yaml
signatures:
  check_descriptions: true
```

Used to detect tool-description poisoning / drift. If enabled, mismatches can produce diagnostics.

---

## Canonical error codes

| Code | Meaning |
|---|---|
| `E_TOOL_DENIED` | Tool matched deny list |
| `E_TOOL_NOT_ALLOWED` | Tool not matched by allow list (when allow list is defined) |
| `E_ARG_SCHEMA` | JSON Schema validation failed |
| `E_TOOL_UNCONSTRAINED` | Tool allowed but has no schema (warn/deny depending on enforcement) |
| `E_RATE_LIMIT` | Rate limit exceeded |
| `E_POLICY_INVALID` | Policy malformed or schema compile error |

---

## Migration (v1 → v2)

Legacy v1 constraints:

```yaml
version: "1.0"
constraints:
  - tool: read_file
    params:
      path:
        matches: "^/workspace/.*"
```

Equivalent v2 schema:

```yaml
version: "2.0"
schemas:
  read_file:
    type: object
    additionalProperties: false
    properties:
      path: { type: string, pattern: "^/workspace/.*", minLength: 1, maxLength: 4096 }
    required: ["path"]
```

---

## CLI commands

```bash
# Validate syntax and compile schemas
assay policy validate policy.yaml

# Migrate v1 policy to v2 (preview)
assay policy migrate --input policy.yaml --dry-run

# Migrate (write)
assay policy migrate --input policy.yaml

# Format (normalize YAML)
assay policy fmt policy.yaml
```
