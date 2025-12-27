# Metrics Reference

Complete documentation for Assay's built-in metrics.

---

## Overview

Metrics are validation functions that check agent behavior. All Assay metrics are:

- **Deterministic** — Same input → same output, always
- **Fast** — Milliseconds, not seconds
- **Binary** — Pass or fail, no floats

---

## Built-in Metrics

| Metric | Purpose | Output |
|--------|---------|--------|
| [`args_valid`](args-valid.md) | Validate tool arguments | Pass/Fail per call |
| [`sequence_valid`](sequence-valid.md) | Validate call order | Pass/Fail per rule |
| [`tool_blocklist`](tool-blocklist.md) | Block forbidden tools | Pass/Fail (count) |

---

## Quick Examples

### args_valid

```yaml
tests:
  - id: check_args
    metric: args_valid
    policy: policies/customer.yaml
```

Validates that all tool arguments match the policy schema.

### sequence_valid

```yaml
tests:
  - id: auth_flow
    metric: sequence_valid
    rules:
      - type: before
        first: authenticate
        then: get_data
```

Validates that tools are called in the correct order.

### tool_blocklist

```yaml
tests:
  - id: no_admin
    metric: tool_blocklist
    blocklist:
      - admin_*
      - delete_*
```

Validates that forbidden tools were never called.

---

## Combining Metrics

A typical test suite uses all three:

```yaml
version: "1"
suite: production-agent

tests:
  # Validate arguments
  - id: args
    metric: args_valid
    policy: policies/all.yaml

  # Validate sequences
  - id: sequences
    metric: sequence_valid
    rules:
      - type: require
        tool: authenticate
      - type: before
        first: authenticate
        then: [read_data, write_data]

  # Block dangerous tools
  - id: blocklist
    metric: tool_blocklist
    blocklist: [delete_*, admin_*, debug_*]
```

---

## Metric Output

All metrics produce structured results:

```json
{
  "id": "args_valid",
  "status": "fail",
  "violations": [
    {
      "tool": "apply_discount",
      "call_index": 15,
      "field": "percent",
      "value": 50,
      "constraint": "max: 30",
      "message": "Value exceeds maximum"
    }
  ],
  "duration_ms": 2
}
```

---

## Why Deterministic?

| Aspect | LLM-as-Judge | Assay Metrics |
|--------|--------------|---------------|
| Consistency | ~85-95% | **100%** |
| Speed | 2-30 seconds | **1-5 ms** |
| Cost | $0.01-$0.10 | **$0.00** |
| Debugging | "Why did it fail?" | Exact violation |
| CI suitability | Poor (flaky) | **Excellent** |

---

## Custom Metrics

See [Custom Metrics](custom.md) for extending Assay with your own validation logic.

---

## See Also

- [Metrics Concept](../concepts/metrics.md)
- [Policies](../config/policies.md)
- [Sequence Rules](../config/sequences.md)
