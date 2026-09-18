# Python SDK

The `assay-it` distribution provides trace recording, validation, and coverage analysis.

## Installation

```bash
pip install assay-it
```

CPython 3.12, 3.13, and 3.14 on macOS x86_64/arm64 and Linux x86_64; other interpreters and platforms are not claimed.

## Quick Start

### Record Traces

```python
from assay import AssayClient

client = AssayClient("traces.jsonl")
client.record_trace({
    "tool": "read_file",
    "args": {"path": "/app/data.json"}
})
```

### Validate

`validate(policy_path, traces)` takes a policy path and a list of traces
(dicts). It returns `Coverage.analyze()` unchanged: a CoverageReport dict.

```python
import json
from assay import validate

with open("traces.jsonl") as f:
    traces = [json.loads(line) for line in f]

report = validate("policy.yaml", traces)
if not report["meets_threshold"]:
    print(report["policy_violations"])
```

### OpenAI Integration

Record tool calls from OpenAI completions:

```python
from assay import TraceWriter, record_chat_completions_with_tools
import openai

client = openai.OpenAI()
writer = TraceWriter("traces/session.jsonl")

result = record_chat_completions_with_tools(
    writer=writer,
    client=client,
    model="gpt-4o",
    messages=[{"role": "user", "content": "Read the config file"}],
    tools=[...],
    tool_executors={"read_file": read_file_fn},
)
```

## Pytest Plugin

Automatic trace capture in tests:

```python
import pytest

@pytest.mark.assay(trace_file="test_traces.jsonl")
def test_agent_workflow():
    # Traces are automatically captured
    pass

@pytest.mark.assay(policy="strict.yaml")
def test_with_policy():
    # Validates against policy after test
    pass
```

Enable in `conftest.py`:

```python
pytest_plugins = ["assay.pytest_plugin"]
```

## Coverage Analysis

```python
import json
from assay import Coverage

with open("traces.jsonl") as f:
    traces = [json.loads(line) for line in f]

coverage = Coverage("policy.yaml")
report = coverage.analyze(traces)

print(f"Coverage: {report['overall_coverage_pct']}%")
print(f"Meets threshold: {report['meets_threshold']}")
print(f"Violations: {report['policy_violations']}")
```

## Evidence Export

```python
from assay import export_evidence

bundle_path = export_evidence(
    profile="profile.yaml",
    output="evidence.tar.gz"
)
```

## API Reference

### `AssayClient`

| Method | Description |
|--------|-------------|
| `record_trace(event)` | Record a tool call event |
| `flush()` | Write pending events to disk |
| `close()` | Close the trace file |

### `validate(policy_path, traces)`

Returns `Coverage.analyze()` unchanged. The example above uses the serialised
CoverageReport field names. There is no separate `passed` / `findings` result
type.

### `TraceWriter`

| Method | Description |
|--------|-------------|
| `write(event)` | Write event to trace file |
| `close()` | Close file handle |

## Framework Integration

### LangChain / LlamaIndex

Use the CLI import command with OpenTelemetry:

```bash
assay trace ingest-otel --input otel-export.jsonl --db .eval/eval.db --out-trace traces.jsonl
```

Or configure callbacks to write directly to `TraceWriter`.

## See Also

- [Quickstart](../getting-started/python-quickstart.md)
- [Trace Format](../concepts/traces.md)
- [Policy Reference](../reference/policies.md)
