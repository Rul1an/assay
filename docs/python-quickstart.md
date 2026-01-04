# Python Quickstart

Use the Assay Python SDK to validate Model Context Protocol (MCP) tool calls in your test suite.

## Installation

```bash
pip install assay
```

## Validate with Pytest

### 1. Define Policy

Create `assay.yaml` to define allowed tools, argument schemas, and sequences:

```yaml
version: 1
tools:
  search_kb:
    args:
      properties:
        query: { minLength: 5 }

  escalate_ticket:
    sequence:
      before: ["search_kb"] # Must search before escalation
```

### 2. Write Test

Create `test_compliance.py`. Load your trace logs (JSONL) and assert coverage:

```python
import json
import pytest
from assay import Coverage

def test_policy_compliance():
    # 1. Load traces (list of tool call dicts)
    with open("traces/latest_run.jsonl") as f:
        traces = [json.loads(line) for line in f]

    # 2. Analyze against policy
    cov = Coverage("assay.yaml")
    report = cov.analyze(traces, min_coverage=90.0)

    # 3. Assert compliance (report is a dict)
    assert report["meets_threshold"], \
        f"Coverage failed: {report['overall_coverage_pct']}% (expected 90%)"

    assert not report["high_risk_gaps"], \
        f"High risk gaps found: {report['high_risk_gaps']}"
```

## Pytest Fixture

The `assay_client` fixture is automatically available. Use it to record traces during live tests.

```python
def test_live_agent(assay_client):
    # Record tool calls during test execution
    assay_client.record_trace({
        "tool": "search_kb",
        "args": {"query": "payment failure"}
    })
```
