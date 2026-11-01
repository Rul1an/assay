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
| `limits` | object | Optional ceilings. Under `assay mcp wrap` both count tool calls only. |
| `signatures` | object | Accepted, with no effect. Nothing reads it. |
| `tool_pins` | map<string, object> | Cryptographic pins for expected tool identity. |
| `discovery` | object | Read by `assay discover`. No effect under `assay mcp wrap`. |
| `runtime_monitor` | object | Read by `assay monitor`. No effect under `assay mcp wrap`. |
| `kill_switch` | object | Read by `assay monitor`. Under `assay mcp wrap` it is only validated at load. |

Unknown fields are ignored. `assay mcp wrap` and `assay policy validate` name them on stderr, along with any key in this table that has no effect under wrap, and both still run.

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

The `_classes` fields read classes from a top-level `tool_classes:` map of tool name to class
names. A tool with no entry has no class, and an unknown top-level key is ignored.

### Wildcards

Assay uses simple `*` wildcards, and the language has exactly five forms:

- `"*"` matches all tools
- `"read_*"` matches by prefix
- `"*_file"` matches by suffix
- `"*search*"` matches by substring
- patterns without `*` are exact matches

In the four wildcard forms a `*` is unbounded: it crosses any character, including the `.` or
`__` a server may use to namespace its tools. `"*"` therefore admits tools that do not exist yet,
including ones a future upstream adds.

**A `*` that is neither the first nor the last character is a literal asterisk.** There is no
fifth wildcard form. `read_*_file` does not match `read_config_file`; it matches only a tool
literally named `read_*_file`. The same goes for several interior stars, such as `a*b*c`, and for
interior stars inside the substring form: `*a*b*` searches for the three characters `a*b`.

**That one fails in the dangerous direction.** In `allow`, an interior-star pattern matches
nothing and the tool is refused, which is visible. In `deny`, it matches nothing and the tool is
**permitted** -- by a line whose author believed it was blocking something. If a `deny` entry
looks like a glob and is not one of the five forms above, it is not blocking what it names.

For the four forms that do expand, **the risk is one-sided and a policy file does not show it.**
A wildcard in `deny` over-blocks, which fails visibly and safely. The same wildcard in `allow`
over-permits, which fails silently. Azure RBAC measured this direction: about half of the 15,481
catalogued actions reach across Resource Providers under non-obvious wildcards, and the
recommendation there was explicit enumeration rather than a more careful pattern language
([arXiv 2506.10755v3](https://arxiv.org/abs/2506.10755)). Prefer naming the tools you mean in
`allow`, and keep `"*"` for the cases where admitting the unknown is the intent rather than the
default.

Two corners, so they are stated rather than discovered. `"**"` (and any longer run of stars) is
the substring form with an empty substring, so it matches everything, exactly as `"*"` does — it
is not the mandate surface's crossing operator. An empty pattern `""` has no star, so it is an
exact match against the empty name and matches no real tool.

> A different pattern language applies to mandate `tool_patterns`, where `*` stops at a `.` and
> `**` crosses it, the way a filesystem glob treats `/`. The two are deliberately different and
> pinned against each other by a test; do not carry a pattern from one surface to the other
> without re-reading it.

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
File and HTTP references are not retrieved during evaluation. Bundle external definitions into the
policy document before execution.

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

These controls sit next to ordinary allow/deny rules, and `assay mcp wrap` enforces them before a
call reaches the server:

```yaml
tools:
  allow: ["read_file", "deploy_release", "create_ticket"]
  approval_required: ["deploy_release"]
  restrict_scope: ["read_file"]
  redact_args: ["create_ticket"]

  restrict_scope_contract:
    scope_type: "resource"
    scope_value: "/workspace"
    scope_match_mode: "prefix"

  redact_args_contract:
    redaction_target: "body"
    redaction_mode: "mask"
    redaction_scope: "args"
```

A refused call gets a typed denial, and the decision log records `P_APPROVAL_REQUIRED`,
`P_RESTRICT_SCOPE` or `P_REDACT_ARGS` with the reason. A contract Assay cannot evaluate, such as an
unsupported `scope_type`, refuses every call it covers. Under `--dry-run` nothing is refused or
changed: the call is forwarded as sent, the record names what would have happened, and no redaction
is recorded as applied.

**`approval_required`** forwards a call only with a valid `_meta.approval` object in its arguments:
`approval_id`, `approver`, `issued_at`, `expires_at`, `scope`, `bound_tool` and `bound_resource`,
current at the time of the call and bound to this tool and this `resource`.

**This is not proof of human approval.** The artifact is not signed, and whoever writes the
arguments can write it, including the agent. It stops a call nobody prepared an approval for, and
it records what was claimed. No host is known to send it today, so without a component of yours that
adds it, `approval_required` refuses every call it covers. The decision event carries a digest and
profile of the artifact under `assay.approval_artifact.structured_meta_jcs.v0`, which is not a
rendered UI view claim, not raw byte retention, and not proof of what a user saw or intended.

**`restrict_scope`** reads the call's `resource` argument, or `_meta.resource`; if both are stated
and differ, the call names no resource and is refused. `scope_type` is `resource`, `tool` or
`tool_class`, and `scope_match_mode` is `exact`, `prefix`, `contains` or `any`.

`any` accepts every stated `resource` and ignores `scope_value`, so it only requires that the call
name one. In the other modes an empty `scope_value` covers no `resource`. For a `resource`, `prefix` means containment: it ends at a `/` boundary, so `/workspace` does not
cover `/workspace-other`, and a `..` segment is refused under either separator, percent-encoded or
not, as is any `%25`. A call with no `resource` is refused, so **a tool whose server reads another
argument, such as `path`, needs a `schemas:` rule for that argument**: this control does not see
it, and a caller can state a `resource` that differs from what the server acts on. Symlinks and
what the server resolves stay outside it.

**`redact_args`** changes the frame the server receives, and leaves other frames byte-identical.
`redaction_target` is `path`, `query`, `headers`, `body`, `metadata` (`arguments._meta`; the
protocol's `params._meta` is forwarded unchanged) or `args` (each top-level value), `redaction_scope` is `request`, `args` or `metadata`, and `redaction_mode` is:

| Mode | The target becomes |
|---|---|
| `mask` | `[REDACTED]` |
| `drop` | removed, without the whitespace between the remaining members; for `args`, an empty object |
| `partial` | its first two characters and `***`; refused if it is not a string |
| `hash` | `sha256:` and the digest of its JSON, a fingerprint with no key, so a guessable value can be recovered from it |

A target the call does not have refuses the call. With `--request-receipts-out`, a receipt holds the
rewritten line. When a call to a tool under `redact_args` was not rewritten, because any rule refused
it or under `--dry-run`, its line is not retained: capture stops for the rest of the run, and stderr
says so. A denial of such a tool leaves out schema violation detail, which quotes the value. With the
`args` or `metadata` target, or when any rule refuses such a tool, the decision log omits the
resource, the approval artifact and the delegation, and names the call by its request id rather than
`_meta.tool_call_id`. Any other denial of such a tool, such as by name, schema or
rate limit, omits them under `--dry-run` too, whatever the target. In a measured run, a call recorded this way no
longer joins its SDK event by `tool_call_id`, and the correlation report says
`sdk_tool_call_without_policy_binding`.

Redaction covers the target you name, not secrets elsewhere in the call, such as a continuation's
`inputResponses`. A `schemas:` rule for a tool outside `redact_args` can quote the rejected value in
the reply and the audit log.

A `tools/call` with no object `params` or no string `name` is answered with `-32602` and never
forwarded: there is no tool to decide on.

## Tool Pins

`tool_pins` protect against tool-definition drift by pinning the expected server, tool name, and hashes:

```yaml
tool_pins:
  read_file:
    server_id: "filesystem-prod"
    tool_name: "read_file"
    # Replace these example values with the full 64-character lowercase SHA-256 hashes.
    schema_hash: "9f4d4d0f9f4d4d0f9f4d4d0f9f4d4d0f9f4d4d0f9f4d4d0f9f4d4d0f9f4d4d0f"
    meta_hash: "42f5df3e42f5df3e42f5df3e42f5df3e42f5df3e42f5df3e42f5df3e42f5df3e"
```

Under `assay mcp wrap`, a call to a pinned tool is forwarded only when this client received a
definition that matches the pin. A definition counts when it answers a `tools/list` this client sent.
The call is denied, and recorded with reason code `P_TOOL_UNOBSERVED`, when:

- the client has not listed the tool in this session, including a tool on a page it did not fetch or
  a tool missing from the list;
- the session received different definitions under one name;
- `notifications/tools/list_changed` arrived and the client has not listed again;
- a line left what the client holds unknown: anything on the server's stdout that is not one readable
  JSON object (a log line, a byte order mark, a batch, duplicate members, a bare carriage return, bytes
  that are not UTF-8), a list reply with both `result` and `error` or with an entry that has no usable
  name, a frame with both `method` and `result`, a `tools` member under any id but the unanswered
  list's, an id equal to that one only as a number, a reply to an older list after a newer list's reply, or
  a page whose cursor is not the one the current list gave.

MCP allows only MCP messages on a server's stdout. A server that logs there between a list and a call
gets its pinned tools refused until the client lists again.

A definition that differs from the pin is denied with `P_TOOL_DRIFT`. A client has to wait for its
first list reply before calling a pinned tool, and one that reuses a list cached from an earlier
session has to list once. A call sent while a later list is unanswered is compared with the list before
it. A list reply already in flight when `list_changed` arrives does not count, so a client
that ignores the notification stays refused until it lists again. Listing again
replaces what was observed, so this compares against the latest list, not every list in the session.
A definition counts as soon as its page arrives, before the list is complete. A client that overlaps
lists and pages can be refused until it lists again. Pages are told apart by cursor, so a server that
gives two lists the same cursor string can have a page of the older list counted toward the newer one.

A pin covers `name`, `inputSchema` (or `input_schema` from a server that sends only that) and
`description`. A difference only in `title`, `annotations`,
`outputSchema` or `_meta` does not drift, and a match means unchanged, not benign.

`schema_hash` is the hex SHA-256 of the RFC 8785 (JCS) canonical bytes of the tool's `inputSchema`,
or of `null` when the tool declares none. `meta_hash` is the hex SHA-256 of the description bytes
verbatim, or of the empty string when there is none. Because `schema_hash` is taken over canonical
bytes, a server that reorders its keys or re-emits `100` as `1e2` does not drift; a changed schema
does, with one exception.

RFC 8785 renders every number as an IEEE 754 double, so two numeric literals with the same double
value become one pin. Below 2^53 that is only a spelling difference and there is nothing to detect.
Above it, two literals can be different integers and still share a double, and then the pin stops
seeing a real edit: a schema changed only from `{"maximum": 9007199254740993}` to
`{"maximum": 9007199254740992}` keeps the same `schema_hash` while admitting a different set of
arguments, because a validator still reads the two literals as different integers. **A `tool_pins`
entry does not detect that edit.**

The exception is a property of the number, not of the keyword, so do not read any keyword list as
the boundary. It applies wherever such a pair of integers is compared against an argument for order
or identity — `maximum`, `minimum`, `exclusiveMaximum`, `exclusiveMinimum`, `const` and `enum` are
where it is measured to bite today, and that is an example set, not a limit. A reader that keeps
exact integers still tells such literals apart at any size, and the pin does not.

An argument constraint written in this policy is unaffected, because it is validated from the
schema you wrote here and not from the one the server declares.

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
