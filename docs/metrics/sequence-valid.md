# sequence_valid

Validate that tool calls follow ordering rules.

---

## Synopsis

```yaml
configVersion: 1
suite: agent_workflows
model: trace
tests:
  - id: auth_flow
    input:
      prompt: "Fetch my account data"
    expected:
      type: sequence_valid
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
| `blocklist` | No tool whose name contains the pattern may be called |
| `max_calls` | Tool may be called at most `max` times |
| `eventually` | Tool must be called within the first `within` calls |
| `never_after` | Once the trigger has been called, the forbidden tool may not be |
| `after` | Each trigger call must be followed by the `then` tool within `within` calls |
| `sequence` | The tools must appear in this order (`strict: true`: consecutively) |

The [Sequence Rules DSL](../reference/config/sequences.md) reference gives each rule's exact
semantics, and how to match a call by its arguments.

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
configVersion: 1
suite: agent_workflows
model: trace
tests:
  - id: secure_workflow
    input:
      prompt: "Sync my data with the external service"
    expected:
      type: sequence_valid
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

`assay run` writes each test's metrics to `run.json`. For this metric the entry is
`results[].details.metrics.sequence_valid`, and it records every rule's outcome, not only the
first failure. A failing run of the synopsis over the trace `[initialize, get_data]`:

```json
{
  "score": 0.0,
  "passed": false,
  "exercised": "exercised",
  "details": {
    "message": "sequence_valid rule failed: tool 'get_data' was found (index 1) but required preceding tool 'authenticate' was missing",
    "rule_evaluations": [
      {
        "rule_id": "before:authenticate->get_data",
        "kind": "before",
        "outcome": "violated",
        "spanned": [1],
        "reason": "tool 'get_data' was found (index 1) but required preceding tool 'authenticate' was missing"
      }
    ]
  }
}
```

Each record's `outcome` is `held`, `violated` or `not_exercised`. A rule whose trigger never
appeared is `not_exercised`. When every rule is, the test still passes, and `exercised` reads
`"not_exercised"`: that field, not the pass, tells a policy that held from one nothing tested.

On stderr, `assay run` names the failing test and exits 1:

```
❌ auth_flow  failed: sequence_valid  (0.0s)
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
