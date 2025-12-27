# Sequence Rules DSL

Define valid tool call sequences with declarative rules.

---

## Overview

The Sequence Rules DSL lets you enforce **order constraints** on tool calls:

- "Always verify identity before deleting a customer"
- "Never call admin tools from untrusted contexts"
- "Read before write"

These rules are **deterministic** — they produce pass/fail results with no ambiguity.

---

## Quick Example

```yaml
# mcp-eval.yaml
tests:
  - id: verify_before_delete
    metric: sequence_valid
    rules:
      - type: before
        first: VerifyIdentity
        then: DeleteCustomer
```

If your agent calls `DeleteCustomer` without first calling `VerifyIdentity`, the test fails.

---

## Rule Types

### `require` — Must Contain

The trace must contain at least one call to the specified tool.

```yaml
rules:
  - type: require
    tool: VerifyIdentity
```

| Trace | Result |
|-------|--------|
| `[GetCustomer, VerifyIdentity, UpdateCustomer]` | ✅ Pass |
| `[GetCustomer, UpdateCustomer]` | ❌ Fail |

---

### `before` — Order Constraint

Tool A must be called before Tool B (at least once).

```yaml
rules:
  - type: before
    first: GetCustomer
    then: UpdateCustomer
```

| Trace | Result |
|-------|--------|
| `[GetCustomer, UpdateCustomer]` | ✅ Pass |
| `[UpdateCustomer, GetCustomer]` | ❌ Fail |
| `[GetCustomer, UpdateCustomer, GetCustomer]` | ✅ Pass |

**Note:** `before` checks that *at least one* call to `first` happens before *the first* call to `then`.

---

### `immediately_before` — Strict Adjacency

Tool A must be called *immediately* before Tool B (no other calls in between).

```yaml
rules:
  - type: immediately_before
    first: ValidateInput
    then: ExecuteAction
```

| Trace | Result |
|-------|--------|
| `[ValidateInput, ExecuteAction]` | ✅ Pass |
| `[ValidateInput, LogEvent, ExecuteAction]` | ❌ Fail |

---

### `blocklist` — Forbidden Tools

These tools must never be called.

```yaml
rules:
  - type: blocklist
    tools:
      - admin_delete
      - system_reset
      - drop_database
```

| Trace | Result |
|-------|--------|
| `[GetCustomer, UpdateCustomer]` | ✅ Pass |
| `[GetCustomer, admin_delete]` | ❌ Fail |

**Glob patterns** are supported:

```yaml
rules:
  - type: blocklist
    tools:
      - admin_*
      - system_*
      - *_dangerous
```

---

### `allowlist` — Only These Tools

Only the specified tools are allowed. Everything else fails.

```yaml
rules:
  - type: allowlist
    tools:
      - GetCustomer
      - UpdateCustomer
      - SendEmail
```

| Trace | Result |
|-------|--------|
| `[GetCustomer, UpdateCustomer]` | ✅ Pass |
| `[GetCustomer, DeleteCustomer]` | ❌ Fail (DeleteCustomer not in allowlist) |

---

### `count` — Call Frequency

Limit how many times a tool can be called.

```yaml
rules:
  - type: count
    tool: SendEmail
    max: 3
```

| Trace | Result |
|-------|--------|
| `[SendEmail, SendEmail]` | ✅ Pass |
| `[SendEmail, SendEmail, SendEmail, SendEmail]` | ❌ Fail |

Options:

```yaml
rules:
  - type: count
    tool: SendEmail
    min: 1      # At least 1
    max: 3      # At most 3
    exact: 2    # Exactly 2 (overrides min/max)
```

---

## Combining Rules

Rules are evaluated with **AND** logic. All rules must pass.

```yaml
tests:
  - id: customer_workflow
    metric: sequence_valid
    rules:
      # Must verify identity
      - type: require
        tool: VerifyIdentity
      
      # Must verify before any destructive action
      - type: before
        first: VerifyIdentity
        then: DeleteCustomer
      
      # Never call admin tools
      - type: blocklist
        tools: [admin_*]
      
      # Max 5 API calls
      - type: count
        tool: ExternalAPI
        max: 5
```

---

## Error Messages

When a rule fails, Assay provides actionable feedback:

```
❌ FAIL: sequence_valid (verify_before_delete)

   Rule: before
   Expected: VerifyIdentity before DeleteCustomer
   Actual: DeleteCustomer called at position 2, but VerifyIdentity never called

   Trace:
     1. GetCustomer
     2. DeleteCustomer  ← violation
     3. SendEmail

   Suggestion: Add VerifyIdentity call before DeleteCustomer
```

---

## Real-World Patterns

### E-commerce: Payment Flow

```yaml
rules:
  # Validate cart before checkout
  - type: before
    first: ValidateCart
    then: ProcessPayment
  
  # Verify inventory before charging
  - type: before
    first: CheckInventory
    then: ProcessPayment
  
  # Never refund more than once
  - type: count
    tool: ProcessRefund
    max: 1
```

### Healthcare: Data Access

```yaml
rules:
  # Always authenticate
  - type: require
    tool: AuthenticateUser
  
  # Authenticate before any data access
  - type: before
    first: AuthenticateUser
    then: GetPatientRecord
  
  # Log all access
  - type: immediately_before
    first: GetPatientRecord
    then: LogAccess
  
  # No admin tools
  - type: blocklist
    tools: [admin_*, system_override]
```

### Agent Handoffs: Multi-Agent

```yaml
rules:
  # Router must run first
  - type: before
    first: RouterAgent
    then: [SpecialistA, SpecialistB, SpecialistC]
  
  # Only one specialist per request
  - type: count
    tool: SpecialistA
    max: 1
  - type: count
    tool: SpecialistB
    max: 1
```

---

## Advanced: Conditional Rules

*(Coming in v1.1)*

```yaml
rules:
  - type: before
    first: VerifyIdentity
    then: DeleteCustomer
    when:
      context.user_role: "standard"  # Only for non-admins
```

---

## Migrating from v0

If you have old-style sequence configs:

```bash
assay migrate --config mcp-eval.yaml
```

This converts:

```yaml
# Old format (v0)
sequences:
  - [GetCustomer, UpdateCustomer]
```

To:

```yaml
# New format (v1)
rules:
  - type: before
    first: GetCustomer
    then: UpdateCustomer
```

---

## Best Practices

### 1. Start Simple

Begin with `blocklist` and `require`, then add `before` rules.

```yaml
rules:
  - type: blocklist
    tools: [admin_*, dangerous_*]
  - type: require
    tool: Authenticate
```

### 2. Use Descriptive IDs

```yaml
tests:
  - id: auth_before_data_access  # ✅ Clear
  - id: test_1                   # ❌ Unclear
```

### 3. Keep Rules Focused

One rule per concern. Don't combine unrelated checks.

### 4. Test the Rules Themselves

Create traces that *should* fail to verify your rules catch violations.

---

## Reference

| Rule Type | Required Fields | Optional Fields |
|-----------|-----------------|-----------------|
| `require` | `tool` | — |
| `before` | `first`, `then` | — |
| `immediately_before` | `first`, `then` | — |
| `blocklist` | `tools` | — |
| `allowlist` | `tools` | — |
| `count` | `tool` | `min`, `max`, `exact` |

---

## See Also

- [Metrics Reference: sequence_valid](../metrics/sequence-valid.md)
- [Policy Files](policies.md)
- [Migration Guide](migration.md)
