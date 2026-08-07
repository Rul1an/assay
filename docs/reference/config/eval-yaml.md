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
        tool: get_weather
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
  input:
    prompt: "..."
  expected:
    type: must_contain
    must_contain: ["..."]
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
| `must_not_contain` | List of substrings that must NOT appear in the response. |
| `regex_match` | Regex pattern the response must match. |
| `json_schema` | Validates the response against a JSON schema. |
| `semantic_similarity_to` | Embedding similarity against a reference answer. |

An `expected:` block must contain exactly one effective output check. Empty checks,
unknown fields, and multiple checks in one block are rejected as config errors.

`sequence: []` is not vacuous: it is the exact constraint that the trace contains
zero tool calls. Explicit empty `rules: []` is rejected unless an effective
`sequence` is also present; a referenced policy does not make empty inline rules
effective.

```yaml
# Accepted: require an exact empty tool-call sequence.
expected:
  type: sequence_valid
  sequence: []

# Rejected: no sequence, policy, or effective rule.
expected:
  type: sequence_valid
  rules: []
```

The tagged V1 form above is preferred. Existing configurations may keep either of
these compatibility forms:

```yaml
# Legacy scalar value
expected:
  must_contain: "Tokyo"

# Legacy list wrapper, with exactly one entry
expected:
  - must_contain: "Tokyo"
```

The historical `type: sequence` form remains readable. A legacy `expected:` list
with more than one entry is rejected because the model can enforce only one output
check; move additional checks to `assertions:` or split them into separate tests.

A test may omit `expected:` when its checks live in `assertions:`. Omitting both is
accepted for compatibility but `assay validate` emits `W_CFG_VACUOUS_EXPECTED`.

### `assertions`

Defines **behavioral** validation (the trace). Replaces the legacy `policies` block.

#### `trace_must_call_tool`
The trace must contain at least one call to the specified tool.
```yaml
- type: trace_must_call_tool
  tool: "calculator"
  min_calls: 1 # optional
```

#### `trace_must_not_call_tool`
The trace must NOT contain any calls to the specified tool.
```yaml
- type: trace_must_not_call_tool
  tool: "system_shutdown"
```

#### `trace_tool_sequence`
Enforces a defined order of operations.
```yaml
- type: trace_tool_sequence
  sequence: ["login", "view_balance", "logout"]
  allow_other_tools: false
```

#### `trace_max_steps`
Limits the number of steps in the trace.
```yaml
- type: trace_max_steps
  max: 8
```

#### `args_valid`
Checks a tool's arguments against a policy. Distinct from the `args_valid` **metric** under
`expected:` — this one asserts over the trace, that one over the response.
```yaml
- type: args_valid
  tool: "transfer_funds"
  test_args: { amount: 100 }   # optional
  policy: { ... }              # optional
  expect: "pass"               # optional
```

#### `sequence_valid`
Checks a tool-call ordering against a sequence policy. Distinct from the `sequence_valid`
**metric** under `expected:`, which is documented above.
```yaml
- type: sequence_valid
  test_trace_raw:              # optional
    - tool: "Authenticate"
      args: {}
  policy: { ... }              # optional
  expect: "pass"               # optional
```

#### `tool_blocklist`
Checks tool calls against a blocklist policy.
```yaml
- type: tool_blocklist
  test_tool_calls: ["delete_all"]   # optional
  policy: { ... }                   # optional
  expect: "pass"                    # optional
```

> Every field above marked optional is optional to the parser, not to the check. An assertion
> whose fields leave it unable to fail is refused by `assay validate`, and by `assay run
> --deny-ineffective-assertions` at load. See #1949.
