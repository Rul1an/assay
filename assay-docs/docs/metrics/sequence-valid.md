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
| `immediately_before` | Tool A must directly precede Tool B |
| `blocklist` | These tools must never be called |
| `allowlist` | Only these tools are allowed |
| `count` | Limit call frequency |

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

### Immediately Before

```yaml
rules:
  - type: immediately_before
    first: validate_input
    then: execute_action
```

### Blocklist

```yaml
rules:
  - type: blocklist
    tools:
      - admin_delete
      - system_reset
```

### Allowlist

```yaml
rules:
  - type: allowlist
    tools:
      - get_customer
      - update_customer
      - send_email
```

### Count

```yaml
rules:
  - type: count
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
        then: [get_data, update_data, delete_data]
      
      # No admin tools
      - type: blocklist
        tools: [admin_*, system_*]
      
      # Max 5 API calls
      - type: count
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

## Glob Patterns

Blocklist and allowlist support globs:

```yaml
rules:
  - type: blocklist
    tools:
      - admin_*       # admin_delete, admin_create, etc.
      - *_dangerous   # delete_dangerous, run_dangerous
      - debug_*       # debug_mode, debug_dump
```

---

## See Also

- [Sequence Rules DSL](../config/sequences.md)
- [args_valid](args-valid.md)
- [tool_blocklist](tool-blocklist.md)
