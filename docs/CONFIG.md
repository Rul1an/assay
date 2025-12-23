# Configuration Reference (v0.7.0)

Verdict supports two configuration styles:
1.  **SDK-Native** (`eval.yaml`): The preferred format for v0.7.0+.
2.  **CLI-Compat** (`verdict.yaml`): Legacy format for backward compatibility.

The SDK automatically detects and normalizes both formats into a single `EvalConfig` model.

---

## 1. SDK-Native Format (`eval.yaml`)

This format is "metric-first", defining a list of metrics to run for each test case.

```yaml
version: 1
suite: default
tests:
  - id: check_weather
    prompt: "What is the weather in Tokyo?"
    metrics:
      - name: regex_match
        params: { pattern: ".*Weather.*" }
      - name: trace_must_call_tool
        params: { tool: "GetWeather" }
      - name: faithfulness
        kind: judge
        threshold: 0.9
```

---

## 2. CLI-Compat Format (`verdict.yaml`)

This format uses `expected` and `assertions` keys. The SDK maps these to `metrics` internally.

```yaml
version: 1
suite: default
tests:
  - id: check_weather
    input:
      prompt: "What is the weather in Tokyo?"
    expected:
      type: regex_match
      pattern: ".*Weather.*"
    assertions:
      - type: trace_must_call_tool
        tool: "GetWeather"
```

---

## 3. Mapping Specification (CLI → SDK)

The following tables define how the SDK normalizes CLI configurations.

### 3.1 Top-Level Fields

| CLI Field | SDK Field | Rule |
| :--- | :--- | :--- |
| `version: 1` | - | Validated but ignored (SDK assumes v1 semantics). |
| `suite` | `suite` | 1:1 mapping. |
| `model: "trace"` | - | Implicit. SDK builtins determine if trace/judge is needed. |
| `tests[].id` | `tests[].id` | 1:1. Required. |
| `tests[].input.prompt` | `tests[].prompt` | 1:1. Used as fallback matcher. |

### 3.2 Expected → Metrics

| CLI `expected.type` | SDK Metric Name | Params | Note |
| :--- | :--- | :--- | :--- |
| `regex_match` | `regex_match` | `{pattern: str}` | Matches final assistant output. |

*Note: `exact_match` and `json_schema` are not yet supported in SDK v0.7.0.*

### 3.3 Assertions → Metrics

| CLI `assertion.type` | SDK Metric Name | Params | Note |
| :--- | :--- | :--- | :--- |
| `trace_must_call_tool` | `trace_must_call_tool` | `{tool: str}` | Checks for `type="tool_call"` event. |
| `trace_must_not_call_tool` | `trace_must_not_call_tool` | `{tool: str}` | Inverted check. |

---

## 4. Matching Policy

Determinism is critical for reproducible builds. The SDK enforces the following matching order:

1.  **Match by ID**: If trace metadata contains `test_id` or `episode_id`, it must match `test.id`.
2.  **Match by Prompt**: Fallback if IDs are missing.
    *   **Strict Mode**: Duplicate prompts raise `ConfigError`.
