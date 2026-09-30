# Python Quickstart

Use **Assay** in Python tests to check declared policy coverage over recorded tool calls. The `assay-it` SDK has a stateless `validate()` helper and a stateful `AssayClient` for explicit trace recording; it does not establish overall agent compliance.

## Installation

```bash
pip install assay-it
```

CPython 3.12, 3.13, and 3.14 on macOS x86_64/arm64 and Linux x86_64; other interpreters and platforms are not claimed.

## Usage

The examples require `pytest` and the native SDK installed in the same Python environment. Run from a writable test directory containing a valid `assay.yaml` policy and `traces.jsonl` with one JSON tool-call object per nonblank line. The policy must match those calls; a missing fixture or a failing threshold is not an SDK installation result. The examples below do not create those two input files.

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
    # Returns Coverage.analyze() unchanged: a CoverageReport dict
    # (meets_threshold, policy_violations, overall_coverage_pct)
    report = validate(
        policy_path="assay.yaml",
        traces=traces
    )

    # 3. Assert coverage threshold and policy violations
    assert report["meets_threshold"], \
        f"Coverage is below threshold {report['threshold']}."
    assert not report["policy_violations"]
```

### 2. Coverage Analysis

If you need deeper inspection (e.g., coverage percentages), use the `Coverage` class.

```python
import json
from assay import Coverage

def test_coverage():
    # Load the same JSONL the validate() example uses.
    with open("traces.jsonl") as f:
        traces = [json.loads(line) for line in f]
    # analyze() needs a list of sessions; it does not wrap a flat list of events
    my_traces = [traces]

    cov = Coverage("assay.yaml")

    # Analyze with a minimum coverage threshold of 90%
    report = cov.analyze(traces=my_traces, min_coverage=90.0)

    # overall_coverage_pct is the coverage percentage; there is no score field
    assert report["overall_coverage_pct"] >= 90.0
```

### 3. Pytest Fixture

Record calls explicitly from a custom fixture. Use a fresh output path for each test: the client appends to existing files and does not automatically capture agent activity.

```python
# conftest.py
import pytest

@pytest.fixture
def assay_client(tmp_path):
    from assay import AssayClient
    return AssayClient(trace_file=str(tmp_path / "live_run.jsonl"))

# test_agent.py
def test_agent_run(assay_client):
    # ... agent logic ...
    assay_client.record_trace({"tool": "search", "args": {"q": "foo"}})
```
