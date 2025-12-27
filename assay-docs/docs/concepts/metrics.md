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

Assay ships with three core metrics:

| Metric | Validates | Output |
|--------|-----------|--------|
| `args_valid` | Tool arguments match schema | Pass/Fail per call |
| `sequence_valid` | Tool call order follows rules | Pass/Fail per rule |
| `tool_blocklist` | Forbidden tools weren't called | Pass/Fail (count) |

All three are **deterministic** — no floats, no subjective thresholds, no LLM-as-judge.

---

## args_valid

Validates that tool arguments conform to your policy schema.

### Basic Usage

```yaml
tests:
  - id: check_all_args
    metric: args_valid
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
tools:
  apply_discount:
    arguments:
      percent:
        type: number
        min: 0
        max: 30
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

### Options

```yaml
tests:
  - id: check_specific_tools
    metric: args_valid
    policy: policies/payments.yaml
    tools: [process_payment, refund]  # Only check these
    
  - id: strict_mode
    metric: args_valid
    policy: policies/all.yaml
    strict: true  # Fail on unknown tools
```

---

## sequence_valid

Validates that tool calls follow ordering rules.

### Basic Usage

```yaml
tests:
  - id: verify_before_delete
    metric: sequence_valid
    rules:
      - type: before
        first: verify_identity
        then: delete_customer
```

### Rule Types

| Type | Description |
|------|-------------|
| `require` | Tool must be called at least once |
| `before` | Tool A must precede Tool B |
| `immediately_before` | Tool A must directly precede Tool B |
| `blocklist` | These tools must never be called |
| `allowlist` | Only these tools are allowed |
| `count` | Limit how many times a tool can be called |

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
    metric: sequence_valid
    rules:
      - type: require
        tool: authenticate
      - type: before
        first: authenticate
        then: [read_data, write_data, delete_data]
      - type: blocklist
        tools: [admin_*, debug_*]
      - type: count
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
    metric: tool_blocklist
    blocklist:
      - delete_database
      - drop_table
      - admin_override
```

### Glob Patterns

Use wildcards to match multiple tools:

```yaml
tests:
  - id: no_admin_tools
    metric: tool_blocklist
    blocklist:
      - admin_*        # Matches admin_delete, admin_create, etc.
      - *_dangerous    # Matches delete_dangerous, run_dangerous
      - debug_*        # Matches debug_mode, debug_dump
```

### Example

**Blocklist:**
```yaml
blocklist:
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
# mcp-eval.yaml
version: "1"
suite: customer-service-agent

tests:
  # Validate all tool arguments
  - id: args_valid
    metric: args_valid
    policy: policies/customer.yaml

  # Enforce authentication flow
  - id: auth_flow
    metric: sequence_valid
    rules:
      - type: require
        tool: authenticate_user
      - type: before
        first: authenticate_user
        then: [get_customer, update_customer]

  # Block dangerous operations
  - id: no_destructive
    metric: tool_blocklist
    blocklist:
      - delete_customer
      - purge_data
      - admin_*

output:
  format: [sarif, junit]
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

Extend Assay with custom metrics in Rust:

```rust
// In assay-metrics crate
use assay_core::{Trace, MetricResult};

pub fn my_custom_metric(trace: &Trace, config: &Config) -> MetricResult {
    // Your validation logic
    let violations = trace.tool_calls()
        .filter(|call| !is_valid(call))
        .collect();
    
    MetricResult {
        status: if violations.is_empty() { Pass } else { Fail },
        violations,
        duration_ms: elapsed,
    }
}
```

Register in `mcp-eval.yaml`:

```yaml
tests:
  - id: custom_check
    metric: my_custom_metric
    config:
      threshold: 0.95
```

---

## See Also

- [args_valid Reference](../metrics/args-valid.md)
- [sequence_valid Reference](../metrics/sequence-valid.md)
- [tool_blocklist Reference](../metrics/tool-blocklist.md)
- [Policies](policies.md)
- [Sequence Rules DSL](../config/sequences.md)
