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

Create `policy.yaml` for the examples:

```yaml
version: "1"
name: sdk-overview
tools:
  allow: [read_file]
```

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

The SDK does not provide an OpenAI completion wrapper. After your application
executes a tool call, record its tool name and arguments with
`AssayClient.record_trace()` as above. Model requests and tool execution remain
application responsibilities.

## Pytest Plugin

Installing `assay-it` registers the pytest plugin. Request the `assay_client`
fixture and record calls explicitly; the marker configures the output file.
It does not automatically capture calls or validate a policy after the test.

```python
import json
import pytest
from assay import validate

@pytest.mark.assay(trace_file="test_traces.jsonl")
def test_agent_workflow(assay_client):
    assay_client.record_trace({"tool": "read_file", "args": {"path": "report.txt"}})
    with open("test_traces.jsonl") as f:
        traces = [json.loads(line) for line in f]
    validation = validate("policy.yaml", traces)
    assert validation["meets_threshold"]
```

This example requires `policy.yaml` to allow `read_file`. The assertion checks
coverage against the default threshold; it is not a whole-action safety verdict.
If pytest plugin autoload is disabled, load it with `pytest -p assay.pytest_plugin`.

## Coverage Analysis

`Coverage.analyze()` takes a list of sessions, each a list of tool-call dicts.
Wrap the single JSONL session in a list, unlike the convenience `validate()` helper.

```python
import json
from assay import Coverage

with open("traces.jsonl") as f:
    traces = [json.loads(line) for line in f]

coverage = Coverage("policy.yaml")
report = coverage.analyze([traces])

print(f"Coverage: {report['overall_coverage_pct']}%")
print(f"Meets threshold: {report['meets_threshold']}")
print(f"Violations: {report['policy_violations']}")
```

## Evidence Export

The Python SDK does not export evidence bundles. Use the
[CLI evidence commands](../reference/cli/evidence.md) for supported bundle workflows.

## API Reference

### `AssayClient`

| Method | Description |
|--------|-------------|
| `record_trace(event)` | Append a JSON-serializable event and flush it to the configured JSONL file |

Pass a trace-file path when constructing the client to enable recording.
The SDK exposes no separate `flush()` or `close()` methods.

### `validate(policy_path, traces)`

Returns `Coverage.analyze()` unchanged. The example above uses the serialised
CoverageReport field names. There is no separate `passed` / `findings` result
type.

## Framework Integration

### LangChain / LlamaIndex

Use the CLI import command with OpenTelemetry:

```bash
assay trace ingest-otel --input otel-export.jsonl --db .eval/eval.db --out-trace traces.jsonl
```

Or have application callbacks call `AssayClient.record_trace()` explicitly.

## See Also

- [Quickstart](../getting-started/python-quickstart.md)
- [Trace Format](../concepts/traces.md)
- [Policy Reference](../reference/policies.md)
