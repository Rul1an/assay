# Metrics

Metrics are pure functions that validate agent behavior — the core of Assay's testing.

---

## What is a Metric?

A **metric** is a validation function that takes a trace and returns pass/fail:

```
Trace + Policy → Metric → Pass | Fail
```

Metrics are:

- **Deterministic** — Same input always produces same output
- **Fast** — Milliseconds, not seconds
- **Composable** — Combine multiple metrics in one test suite

---

## Built-in Metrics

Assay ships with deterministic metrics for both classic eval checks and MCP-specific integrity checks.

### Core Eval Metrics

| Metric | Validates | Output |
|--------|-----------|--------|
| `args_valid` | Tool arguments match schema | Pass/Fail per call |
| `sequence_valid` | Tool call order follows rules | Pass/Fail per rule |
| `tool_blocklist` | Forbidden tools weren't called | Pass/Fail (count) |

All three are **deterministic** — no floats, no subjective thresholds, no LLM-as-judge.

### MCP Integrity Metrics

These metrics are useful when your traces include MCP tool-definition metadata from a proxy or trace capture layer.

| Metric | Validates | Key config |
|--------|-----------|------------|
| `tool_description_integrity` | Tool descriptions and input schemas stay pinned or unchanged across snapshots | `pinned_tools` |
| `tool_output_valid` | Tool results match per-tool JSON Schemas | `schemas` |
| `tool_collision_detect` | The same tool name is not registered by multiple MCP servers unless explicitly trusted | `trusted_servers` |

#### `tool_description_integrity`

Use pinned mode when you want to compare a trace against operator-approved tool definitions:

```yaml
tests:
  - id: tool_pins
    input:
      prompt: "Read the project README"
    expected:
      type: tool_description_integrity
      pinned_tools:
        - name: read_file
          description: Reads a file from disk
          schema_sha256: "4d8d8f0d7d4a0f4b9f47d59a3cce5d6e8f4e2b5a1a5ef9e8e2b5e8c6d2a4d901"
```

Leave `pinned_tools` empty to switch to snapshot mode. In that mode, Assay compares multiple `tools/list` snapshots in the same trace and flags mid-session mutations.

#### `tool_output_valid`

`tool_output_valid` complements `args_valid`: it validates what the tool returns, not just what the agent sent.

```yaml
tests:
  - id: tool_outputs
    input:
      prompt: "Run the test suite"
    expected:
      type: tool_output_valid
      schemas:
        exec:
          type: object
          required: [exit_code]
          properties:
            exit_code: { type: integer }
            stdout: { type: string }
```

#### `tool_collision_detect`

Use `trusted_servers` when duplicate tool names are expected only inside a known MCP cluster.

```yaml
tests:
  - id: no_tool_shadowing
    input:
      prompt: "Search the docs for the release notes"
    expected:
      type: tool_collision_detect
      trusted_servers:
        - trusted-fileserver
        - trusted-search
```

---

## args_valid

Validates that tool arguments conform to your policy schema.

### Basic Usage

```yaml
tests:
  - id: check_all_args
    input:
      prompt: "Give customer 42 a discount"
    expected:
      type: args_valid
      policy: policies/customer-service.yaml
```

### What It Checks

For each tool call in the trace:

1. Is the tool defined in the policy?
2. Are required arguments present?
3. Do argument types match?
4. Do values satisfy constraints (min, max, pattern, etc.)?

### Example

**Policy:**
```yaml
version: "2.0"
schemas:
  apply_discount:
    type: object
    properties:
      percent:
        type: number
        minimum: 0
        maximum: 30
    required: [percent]
```

**Trace:**
```json
{"type":"tool_call","tool":"apply_discount","arguments":{"percent":50}}
```

**Result:**
```
❌ FAIL: args_valid

   Tool: apply_discount
   Argument: percent = 50
   Violation: Value exceeds maximum (max: 30)
```

### Inline Schema

The schemas can live in the test instead of a policy file, one JSON Schema per tool:

```yaml
tests:
  - id: payment_args
    input:
      prompt: "Refund order 42"
    expected:
      type: args_valid
      schema:
        refund:
          type: object
          properties:
            amount: { type: number, minimum: 0 }
          required: [amount]
```

---

## sequence_valid

Validates that tool calls follow ordering rules.

### Basic Usage

```yaml
tests:
  - id: verify_before_delete
    input:
      prompt: "Delete customer 42"
    expected:
      type: sequence_valid
      rules:
        - type: before
          first: verify_identity
          then: delete_customer
```

### Rule Types

| Type | Description |
|------|-------------|
| `require` | Tool must be called at least once |
| `eventually` | Tool must be called within N steps of the start of the trace |
| `max_calls` | Tool may be called at most a number of times |
| `before` | Tool A must precede Tool B |
| `after` | Tool B must be called within N steps after Tool A |
| `never_after` | Tool B must not be called after Tool A |
| `sequence` | These tools must be called in this order |
| `blocklist` | These tools must never be called |

The [Sequence Rules DSL](../reference/config/sequences.md) gives each rule's fields.

### Example

**Rules:**
```yaml
rules:
  - type: require
    tool: authenticate
  - type: before
    first: authenticate
    then: get_patient_record
```

**Trace:**
```json
{"type":"tool_call","tool":"get_patient_record","arguments":{}}
```

**Result:**
```
❌ FAIL: sequence_valid

   Rule: require
   Expected: authenticate to be called
   Actual: authenticate was never called

   Rule: before
   Expected: authenticate before get_patient_record
   Actual: get_patient_record called without prior authenticate
```

### Combining Rules

Rules are evaluated with AND logic — all must pass:

```yaml
tests:
  - id: secure_workflow
    input:
      prompt: "Export this month's report"
    expected:
      type: sequence_valid
      rules:
        - type: require
          tool: authenticate
        - type: before
          first: authenticate
          then: read_data
        - type: blocklist
          pattern: admin_
        - type: max_calls
          tool: api_call
          max: 10
```

---

## tool_blocklist

Validates that forbidden tools were never called.

### Basic Usage

```yaml
tests:
  - id: no_dangerous_tools
    input:
      prompt: "Clean up the staging environment"
    expected:
      type: tool_blocklist
      blocked:
        - delete_database
        - drop_table
        - admin_override
```

### Exact Names

`tool_blocklist` compares each call against the list by exact name. Wildcards are not
interpreted -- `admin_*` matches only a tool literally called `admin_*`.

```yaml
tests:
  - id: no_admin_tools
    input:
      prompt: "Remove the test accounts"
    expected:
      type: tool_blocklist
      blocked:
        - admin_delete
        - admin_create
        - run_dangerous
```

### Example

**Blocklist:**
```yaml
blocked:
  - admin_delete
```

**Trace:**
```json
{"type":"tool_call","tool":"admin_delete","arguments":{"id":"123"}}
```

**Result:**
```
❌ FAIL: tool_blocklist

   Violation: Blocked tool called
   Tool: admin_delete
   Policy: Blocklist includes 'admin_delete'

   Calls found: 1
```

---

## Combining Metrics

A test suite typically combines all three:

```yaml
# eval.yaml
version: 1
suite: customer-service-agent
model: gpt-4o-mini

tests:
  # Validate all tool arguments
  - id: args_valid
    input:
      prompt: "Give customer 42 a discount"
    expected:
      type: args_valid
      policy: policies/customer.yaml

  # Enforce authentication flow
  - id: auth_flow
    input:
      prompt: "Show me customer 42"
    expected:
      type: sequence_valid
      rules:
        - type: require
          tool: authenticate_user
        - type: before
          first: authenticate_user
          then: get_customer

  # Block dangerous operations
  - id: no_destructive
    input:
      prompt: "Clean up inactive customers"
    expected:
      type: tool_blocklist
      blocked:
        - delete_customer
        - purge_data
```

---

## Metric Output

All metrics produce structured results:

```json
{
  "metric": "args_valid",
  "status": "fail",
  "violations": [
    {
      "tool": "apply_discount",
      "argument": "percent",
      "value": 50,
      "constraint": "max: 30",
      "policy_line": 12
    }
  ],
  "duration_ms": 2
}
```

This feeds into:
- **SARIF** — GitHub Code Scanning annotations
- **JUnit** — CI test result reports
- **JSON** — Programmatic access

---

## Why Deterministic?

Assay metrics are intentionally deterministic (no LLM-as-judge):

| Aspect | LLM-as-Judge | Assay Metrics |
|--------|--------------|---------------|
| Consistency | ~85-95% | **100%** |
| Speed | 2-30 seconds | **1-5 ms** |
| Cost | $0.01-$0.10 | **$0.00** |
| CI suitability | Poor (flaky) | **Excellent** |
| Debugging | Hard (why did it fail?) | **Clear (exact violation)** |

For subjective evaluation ("Is this response helpful?"), use LLM-as-judge in **development**. For CI gates, use deterministic metrics.

---

## Custom Metrics (Advanced)

A metric is a type in the code, not something an eval config registers. Each one implements the
`Metric` trait in `assay-core` (`metrics_api`): it has a name and an `evaluate` over the test case,
its `expected:` block and the response. A new metric is a new `expected:` type, added to Assay and
released with it. An `expected:` block with a type Assay does not have is refused when the config
loads.

---

## See Also

- [args_valid Reference](../metrics/args-valid.md)
- [sequence_valid Reference](../metrics/sequence-valid.md)
- [tool_blocklist Reference](../metrics/tool-blocklist.md)
- [Policies](policies.md)
- [Sequence Rules DSL](../reference/config/sequences.md)
