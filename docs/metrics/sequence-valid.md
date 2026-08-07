# sequence_valid

Validate that tool calls follow ordering rules.

---

## Synopsis

```yaml
tests:
  - id: auth_flow
    metric: sequence_valid
    rules:
      - type: before
        first: authenticate
        then: get_data
```

---

## Description

The `sequence_valid` metric checks that tools are called in the correct order. It validates:

- Required tools are called
- Prerequisite tools run before dependent tools
- Forbidden tools are never called
- Call counts are within limits

---

## Rule Types

| Type | Description |
|------|-------------|
| `require` | Tool must be called at least once |
| `before` | Tool A must precede Tool B |
| `blocklist` | These tools must never be called |

---

## Examples

### Require

```yaml
rules:
  - type: require
    tool: authenticate
```

### Before

```yaml
rules:
  - type: before
    first: get_customer
    then: update_customer
```

### Blocklist

```yaml
rules:
  - type: blocklist
    pattern: admin_
```

### Max Calls

```yaml
rules:
  - type: max_calls
    tool: send_email
    max: 3
```

---

## Combining Rules

Rules are evaluated with AND logic:

```yaml
tests:
  - id: secure_workflow
    metric: sequence_valid
    rules:
      # Must authenticate
      - type: require
        tool: authenticate

      # Auth before data access
      - type: before
        first: authenticate
        then: get_data

      # No admin tools
      - type: blocklist
        pattern: admin_

      # Max 5 API calls
      - type: max_calls
        tool: external_api
        max: 5
```

---

## Output

### Pass

```json
{
  "id": "auth_flow",
  "metric": "sequence_valid",
  "status": "pass",
  "rules_checked": 3,
  "duration_ms": 1
}
```

### Fail

```json
{
  "id": "auth_flow",
  "metric": "sequence_valid",
  "status": "fail",
  "violations": [
    {
      "rule": "before",
      "expected": "authenticate before get_data",
      "actual": "get_data called at position 1, authenticate never called",
      "trace_position": 1
    }
  ],
  "duration_ms": 1
}
```

---

## Error Messages

```
❌ FAIL: sequence_valid (auth_flow)

   Rule: before
   Expected: authenticate before get_data
   Actual: get_data called at position 2, but authenticate never called

   Trace:
     1. initialize
     2. get_data  ← violation
     3. update_data
     4. send_email

   Suggestion: Add authenticate call before get_data
```

---

## Substring Matching

`blocklist` matches by plain substring, not by glob. A `*` is matched literally and will
usually match nothing:

```yaml
rules:
  - type: blocklist
    pattern: debug_        # matches debug_mode, debug_dump
```

---

## See Also

- [Sequence Rules DSL](../reference/config/sequences.md)
- [args_valid](args-valid.md)
- [tool_blocklist](tool-blocklist.md)
