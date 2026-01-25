# Python Quickstart

Integrate **Assay** into your Python test suite to enforce agent compliance. We provide a stateless SDK (`assay-it`) that runs natively in your `pytest` environment.

## Installation

```bash
pip install assay-it
```

## Usage

### 1. Stateless Validation

The `validate()` function is the primary entrypoint. It takes a policy path and a list of traces (dicts).

```python
import json
import pytest
from assay import validate

def test_compliance():
    # 1. Load your agent's trace logs
    with open("traces.jsonl") as f:
        traces = [json.loads(line) for line in f]

    # 2. Validate against your policy
    # Returns a rich report dict (passed, violations, score)
    report = validate(
        policy_path="assay.yaml",
        traces=traces
    )

    # 3. Assert success
    assert report["passed"], \
        f"Compliance Failed! Found {len(report['violations'])} violations."
```

### 2. Coverage Analysis

If you need deeper inspection (e.g., coverage percentages), use the `Coverage` class.

```python
from assay import Coverage

def test_coverage():
    cov = Coverage("assay.yaml")

    # Analyze with a minimum coverage threshold of 90%
    report = cov.analyze(traces=my_traces, min_coverage=90.0)

    assert report["score"] >= 90.0
```

### 3. Pytest Fixture

For live capture during tests, `assay-it` plays nice with custom fixtures.

```python
# conftest.py
@pytest.fixture
def assay_client():
    from assay import AssayClient
    return AssayClient(trace_file="live_run.jsonl")

# test_agent.py
def test_agent_run(assay_client):
    # ... agent logic ...
    assay_client.record_trace({"tool": "search", "args": {"q": "foo"}})
```
