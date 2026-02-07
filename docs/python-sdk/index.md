# Python SDK

The `assay` package provides trace recording, validation, and coverage analysis.

## Installation

```bash
pip install assay
```

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

```python
from assay import validate

result = validate("policy.yaml", "traces.jsonl")
if not result["passed"]:
    for finding in result["findings"]:
        print(f"{finding['level']}: {finding['message']}")
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
from assay import Coverage

coverage = Coverage("policy.yaml", "traces.jsonl")
report = coverage.analyze()

print(f"Coverage: {report['percent']}%")
print(f"Covered tools: {report['covered']}")
print(f"Missing: {report['uncovered']}")
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

### `validate(policy, traces)`

Returns:
```python
{
    "passed": bool,
    "findings": [
        {"level": "error", "rule": "...", "message": "..."}
    ]
}
```

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
