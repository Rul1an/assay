# args_valid

Validate that tool arguments conform to policy schemas.

---

## Synopsis

```yaml
tests:
  - id: validate_args
    metric: args_valid
    policy: policies/customer.yaml
```

---

## Description

The `args_valid` metric checks every tool call in a trace against your policy definitions. It validates:

- Argument types (string, number, boolean, etc.)
- Value constraints (min, max, pattern, enum)
- Required fields
- Nested object/array structures

---

## Options

| Option | Type | Description |
|--------|------|-------------|
| `policy` | string | Path to policy file |
| `tools` | array | Only validate these tools (optional) |
| `strict` | boolean | Fail on unknown tools (default: false) |

---

## Examples

### Basic Usage

```yaml
tests:
  - id: validate_all
    metric: args_valid
    policy: policies/all.yaml
```

### Specific Tools Only

```yaml
tests:
  - id: validate_payments
    metric: args_valid
    policy: policies/payments.yaml
    tools:
      - process_payment
      - refund
      - apply_coupon
```

### Inline Policy

```yaml
tests:
  - id: discount_check
    metric: args_valid
    tool: apply_discount
    constraints:
      percent:
        type: number
        min: 0
        max: 30
```

### Strict Mode

```yaml
tests:
  - id: strict_validation
    metric: args_valid
    policy: policies/known-tools.yaml
    strict: true  # Fail if trace contains unknown tools
```

---

## What Gets Checked

For each tool call in the trace:

1. **Is the tool defined?** (unless `strict: false`)
2. **Are required arguments present?**
3. **Do types match?**
4. **Do values satisfy constraints?**

### Example Policy

```yaml
tools:
  apply_discount:
    arguments:
      percent:
        type: number
        min: 0
        max: 30
      order_id:
        type: string
        required: true
        pattern: "^ord_[0-9]+$"
```

### Example Trace Call

```json
{"tool": "apply_discount", "arguments": {"percent": 50, "order_id": "ord_123"}}
```

### Result

```
❌ FAIL: args_valid

   Tool: apply_discount
   Argument: percent = 50
   Violation: Value exceeds maximum (max: 30)
   Policy: policies/discounts.yaml:8
```

---

## Output

### Pass

```json
{
  "id": "validate_all",
  "metric": "args_valid",
  "status": "pass",
  "violations": [],
  "stats": {
    "calls_checked": 47,
    "tools_checked": 5
  },
  "duration_ms": 2
}
```

### Fail

```json
{
  "id": "validate_all",
  "metric": "args_valid",
  "status": "fail",
  "violations": [
    {
      "call_index": 15,
      "tool": "apply_discount",
      "field": "percent",
      "value": 50,
      "constraint": "max: 30",
      "policy_file": "policies/discounts.yaml",
      "policy_line": 8
    }
  ],
  "stats": {
    "calls_checked": 47,
    "violations_found": 1
  },
  "duration_ms": 3
}
```

---

## Error Messages

Assay provides actionable error messages:

```
❌ FAIL: args_valid (validate_all)

   Tool: apply_discount
   Call: #15 of 47
   Argument: percent = 50
   Violation: Value exceeds maximum (max: 30)
   Policy: policies/discounts.yaml:8

   Suggestion: Use percent <= 30

   Trace location: traces/golden.jsonl:29
```

---

## Common Patterns

### Type Coercion Issues

```
Violation: Expected number, got string
Argument: quantity = "5"
```

Fix: Ensure arguments are correct types.

### Missing Required Fields

```
Violation: Missing required argument
Argument: order_id (not provided)
```

Fix: Add the missing argument or update policy.

### Pattern Mismatch

```
Violation: Value does not match pattern
Argument: code = "abc"
Pattern: ^[A-Z]{6}$
```

Fix: Ensure value matches expected format.

---

## See Also

- [Policies](../config/policies.md)
- [sequence_valid](sequence-valid.md)
- [tool_blocklist](tool-blocklist.md)
