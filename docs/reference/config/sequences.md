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
# eval.yaml
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


### `blocklist` — Forbidden Tools

These tools must never be called.

```yaml
rules:
  - type: blocklist
    pattern: admin_delete
```

| Trace | Result |
|-------|--------|
| `[GetCustomer, UpdateCustomer]` | ✅ Pass |
| `[GetCustomer, admin_delete]` | ❌ Fail |

**Glob patterns** are supported:

```yaml
rules:
  - type: blocklist
    pattern: admin_
```

---


### `max_calls` — Call Frequency Limit

Limit how many times a tool can be called (formerly `count`).

```yaml
rules:
  - type: max_calls
    tool: SendEmail
    max: 3
```

| Trace | Result |
|-------|--------|
| `[SendEmail, SendEmail]` | ✅ Pass |
| `[SendEmail, SendEmail, SendEmail, SendEmail]` | ❌ Fail |

---

### `eventually` — Temporal Deadline

A tool must be called within `N` steps of the start of the trace.

```yaml
rules:
  - type: eventually
    tool: ValidateOutput
    within: 5
```

| Trace | Result |
|-------|--------|
| `[Step1, ..., Step4, ValidateOutput]` | ✅ Pass |
| `[Step1, ..., Step10]` (no validation) | ❌ Fail |

---

### `never_after` — Forbidden Transition

A forbidden tool must never be called after a trigger tool has been used.

```yaml
rules:
  - type: never_after
    trigger: CommitTransaction
    forbidden: ModifyData
```

| Trace | Result |
|-------|--------|
| `[ModifyData, CommitTransaction]` | ✅ Pass |
| `[CommitTransaction, ModifyData]` | ❌ Fail |

---

### `after` — Dependent Deadline

Tool B must be called within `N` steps **after** Tool A occurred.

```yaml
rules:
  - type: after
    trigger: OpenFile
    then: CloseFile
    within: 10
```

| Trace | Result |
|-------|--------|
| `[OpenFile, Read, CloseFile]` | ✅ Pass |
| `[OpenFile, ..., (10 steps), ...]` | ❌ Fail |

---

### `sequence` — Exact Ordering

The named tools must appear in the given order. With `strict: false` (the default) other
tools may appear between them; with `strict: true` they must be consecutive.

```yaml
rules:
  - type: sequence
    tools:
      - Authenticate
      - LoadRecord
      - WriteRecord
    strict: false   # optional
```

| Trace | `strict: false` | `strict: true` |
|-------|-----------------|----------------|
| `[Authenticate, LoadRecord, WriteRecord]` | ✅ Pass | ✅ Pass |
| `[Authenticate, Audit, LoadRecord, WriteRecord]` | ✅ Pass | ❌ Fail |
| `[LoadRecord, Authenticate, WriteRecord]` | ❌ Fail | ❌ Fail |

A trace that never calls any named tool does not exercise this rule; a trace that starts the
sequence and ends part-way through fails it, because the ordering it asserts was not completed.

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
        pattern: admin_

      # Max 5 API calls
      - type: max_calls
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
  - type: max_calls
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
  - type: after
    trigger: GetPatientRecord
    then: LogAccess
    within: 1

  # No admin tools
  - type: blocklist
    pattern: admin_
```

### Agent Handoffs: Multi-Agent

```yaml
rules:
  # Router must run first
  - type: before
    first: RouterAgent
    then: SpecialistA

  # Only one specialist per request
  - type: max_calls
    tool: SpecialistA
    max: 1
  - type: max_calls
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
assay migrate --config eval.yaml
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
    pattern: admin_
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
| `blocklist` | `pattern` | — |
| `max_calls` | `tool`, `max` | — |
| `eventually` | `tool`, `within` | — |
| `never_after` | `trigger`, `forbidden` | — |
| `after` | `trigger`, `then`, `within` | — |
| `sequence` | `tools` | `strict` |

---

## See Also

- [Metrics Reference: sequence_valid](../../metrics/sequence-valid.md)
- [Policy Files](policies.md)
- [Migration Guide](migration.md)
