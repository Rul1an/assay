# Error Handling & Fail-Safe Configuration

Assay can operate in two error-handling modes. This page explains when to use each and how to configure them.

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

### In Streaming Mode (`assay-mcp-server`)

| Scenario | `on_error: block` | `on_error: allow` |
|----------|-------------------|-------------------|
| Check passes | → Allow action | → Allow action |
| Check fails | → Block action | → Block action |
| Check errors | → Block action | → Allow action |

---

## Audit Trail

Regardless of mode, all errors are logged:

```json
{
  "event": "policy_check_error",
  "test_id": "discount_check",
  "error": "Schema parse failed: invalid regex",
  "action_taken": "blocked",  // or "allowed"
  "on_error_mode": "block",
  "timestamp": "2025-12-28T10:30:00Z"
}
```

Use these logs to:
1. Monitor error rates
2. Debug configuration issues
3. Demonstrate compliance (errors were handled correctly)

---

## Decision Framework

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
