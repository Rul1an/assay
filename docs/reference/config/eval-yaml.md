# Configuration Reference (V1)

Assay v0.9.0 introduces a stricter, more declarative **V1 configuration schema**.

```yaml
version: 1 # Required for V1 schema
model: "gpt-4o" # Default model

tests:
  - id: example_test
    input:
      prompt: "What is the weather in Tokyo?"
    expected:
      type: must_contain
      must_contain: ["Tokyo"]
    assertions:
      - type: trace_must_call_tool
        tool_name: get_weather
```

## Top-Level Fields

| Field | Type | Description |
|---|---|---|
| `version` | `integer` | Schema version. Must be `1` for the features below. |
| `model` | `string` | Default model ID for tests that don't specify one. |
| `tests` | `list` | List of test cases. |
| `settings` | `object` | Global execution settings (timeout, concurrency). |

---

## Test Case

Each test in the `tests` list defines a scenario and its validation rules.

```yaml
- id: my_test_id
  description: "Optional description (ignored by runner)"
  input:
    prompt: "..."
  expected:
    type: json_match
    # ...
  assertions: []
```

### `input`

Defines what is sent to the agent.

| Field | Type | Description |
|---|---|---|
| `prompt` | `string` | The user message content. |
| `context` | `string` | Optional system context or preamble. |

### `expected`

Defines the **output** validation (the final answer).

| Type | Description |
|---|---|
| `must_contain` | List of substrings that must appear in the response. |
| `regex_match` | Regex pattern the response must match. |
| `json_match` | Validates response against a JSON schema. |
| `exact_match` | Full string equality check. |

### `assertions`

Defines **behavioral** validation (the trace). Replaces the legacy `policies` block.

#### `trace_must_call_tool`
The trace must contain at least one call to the specified tool.
```yaml
- type: trace_must_call_tool
  tool_name: "calculator"
```

#### `trace_no_tool_call`
The trace must NOT contain any calls to the specified tool.
```yaml
- type: trace_no_tool_call
  tool_name: "system_shutdown"
```

#### `trace_tool_args_match`
Validates that *every* call to a tool matches specific argument values.
```yaml
- type: trace_tool_args_match
  tool_name: "discount"
  args:
    percent: 10
```

#### `trace_tool_args_schema`
Validates tool arguments against a JSON schema.
```yaml
- type: trace_tool_args_schema
  tool_name: "search"
  schema:
    required: ["query"]
    properties:
      query: { type: "string", minLength: 3 }
```

#### `trace_tool_sequence`
Enforces a defined order of operations.
```yaml
- type: trace_tool_sequence
  sequence: ["login", "view_balance", "logout"]
```

#### `trace_no_tool_errors`
Passes only if the trace contains zero tool execution errors.
```yaml
- type: trace_no_tool_errors
```

#### `trace_tool_call_count`
Validates the number of times a tool was called.
```yaml
- type: trace_tool_call_count
  tool_name: "search"
  min: 1
  max: 3
```
