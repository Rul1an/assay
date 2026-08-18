# Error Handling & Fail-Safe Configuration

`assay run` can operate in two error-handling modes. The built-in stdio MCP
server has a separate, always-fail-closed boundary.

## The Problem

What happens when Assay encounters an error during a policy check?

- Network timeout to MCP server
- Malformed trace data
- Schema parsing failure
- Unexpected exception in validation logic

The answer depends on your risk tolerance.

## Two Modes

### `block` (Default) - Fail-Closed

When an error occurs, **deny the action**.

```yaml
settings:
  on_error: block
```

**Behavior:**
- Error during check → Action is blocked
- Guardrail is always enforced
- Errors are surfaced immediately

**Use when:**
- Compliance requirements mandate fail-safe behavior
- You're in a safety-critical environment
- False negatives are worse than false positives

**Tradeoff:** May block legitimate actions if Assay has issues.

### `allow` - Fail-Open

When an error occurs, **permit the action**.

```yaml
settings:
  on_error: allow
```

**Behavior:**
- Error during check → Action is allowed
- Errors are logged but don't block execution
- Agent continues operating

**Use when:**
- Availability is more important than enforcement
- You're in development/testing
- You have other layers of defense

**Tradeoff:** May allow dangerous actions if Assay has issues.

---

## Configuration

### Global Setting

Apply to all checks in a suite:

```yaml
configVersion: 1
suite: my-agent

settings:
  on_error: block  # or: allow

tests:
  - id: test_1
    # ...
```

### Per-Test Override

Override for specific critical tests:

```yaml
settings:
  on_error: allow  # Global: permissive

tests:
  - id: normal_check
    # Inherits: allow

  - id: critical_safety_check
    on_error: block  # Override: strict for this test
    assertions:
      - type: tool_blocklist
        blocked: [DeleteDatabase]
```

### Per-Assertion Override (v1.1+)

Fine-grained control at assertion level:

```yaml
tests:
  - id: multi_check
    assertions:
      - type: args_valid
        on_error: block  # Critical
        tool: ApplyDiscount

      - type: sequence_valid
        on_error: allow  # Less critical
        rules: [...]
```

---

## Runtime Behavior

### In Batch Mode (`assay run`)

| Scenario | `on_error: block` | `on_error: allow` |
|----------|-------------------|-------------------|
| Check passes | ✓ Pass | ✓ Pass |
| Check fails | ✗ Fail | ✗ Fail |
| Check errors | ✗ Error (blocks CI) | ⚠ Warn (CI continues) |

### In the stdio MCP server (`assay-mcp-server`)

Suite, test, and assertion `on_error` settings are not MCP server settings.
The five built-in policy tools fail closed when dispatch fails or times out:

- the tool payload has `allowed: false`;
- the MCP `CallToolResult` has `isError: true`;
- dispatch failures use `E_INTERNAL` and a fixed message;
- timeouts use `E_TIMEOUT` and a fixed message.

Caller-supplied `arguments.on_error` has no authority and is absent from the
advertised tool schemas. Use bounded client retry, supervised restart, and
alerting for availability; do not convert an unavailable policy decision into
permission.

---

## Audit Trail

`assay-mcp-server` emits structured stderr events with a request id, duration,
outcome, and reason code. Public failure messages do not include caller values
or raw internal errors. For example, a timeout result contains:

```json
{
  "allowed": false,
  "error": {
    "code": "E_TIMEOUT",
    "message": "Tool execution timed out"
  }
}
```

Use structured results and logs to:
1. Monitor error rates
2. Debug configuration issues
3. Verify that the fixed MCP fail-closed payload (`allowed: false`, `isError: true`) was published

---

## Decision Framework

This tree applies only to `assay run` suite `settings.on_error`.
It is not an MCP server setting. The stdio MCP server always fail-closes with
a fixed payload; see [In the stdio MCP server](#in-the-stdio-mcp-server-assay-mcp-server).

```
Is this a regulated/compliance environment?
  └─ Yes → on_error: block
  └─ No
      └─ Is this production?
          └─ Yes → on_error: block (probably)
          └─ No
              └─ Is availability critical?
                  └─ Yes → on_error: allow
                  └─ No → on_error: block
```

## Best Practices

1. **Default to `block`** - It's the safer choice
2. **Use `allow` sparingly** - Only where you have defense in depth
3. **Monitor error rates** - High error rates indicate config problems
4. **Test both modes** - Verify your agent handles blocks gracefully
5. **Document your choice** - Compliance auditors will ask

---

## Example: Tiered Configuration

A realistic production setup with layered risk management:

```yaml
configVersion: 1
suite: production-agent

settings:
  on_error: block  # Default: strict

tests:
  # Tier 1: Safety-critical (always block)
  - id: no_database_deletion
    tags: [tier-1, safety]
    on_error: block
    assertions:
      - type: tool_blocklist
        blocked: [DeleteDatabase, DropTable]

  # Tier 2: Business logic (block)
  - id: discount_limits
    tags: [tier-2, business]
    on_error: block
    assertions:
      - type: args_valid
        tool: ApplyDiscount
        schema:
          properties:
            percent: { maximum: 30 }

  # Tier 3: Convenience checks (allow on error)
  - id: response_format
    tags: [tier-3, quality]
    on_error: allow  # Non-critical
    assertions:
      - type: args_valid
        tool: FormatResponse
        schema:
          properties:
            format: { enum: [json, markdown, plain] }
```

This ensures:
- Tier 1 failures always block (even if Assay errors)
- Tier 2 failures block but error-tolerance varies
- Tier 3 is "best effort" - errors don't disrupt the agent
