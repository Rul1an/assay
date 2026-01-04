# Python Quickstart

Assay provides a native Python SDK for integrating Model Context Protocol (MCP) validation directly into your existing test suites (Pytest, Unittest).

## Installation

```bash
pip install assay
```

## Core Concepts

*   **Policy**: A YAML file defining allowed tools, argument schemas, and sequences.
*   **Trace**: A JSONL file containing recorded MCP tool calls (or OpenTelemetry logs).
*   **Coverage**: A metric indicating how well your traces exercise the allowed policy.

## Usage with Pytest

The simplest way to use Assay is via the `assay.pytest` plugin or direct wrapper usage.

### 1. Create a Policy

Define your expectations in `assay.yaml`:

```yaml
version: 1
tools:
  search_kb:
    args:
      properties:
        query: { minLength: 5 }

  escalate_ticket:
    sequence:
      before: ["search_kb"] # Must search first
```

### 2. Write a Test

Create `test_agent.py`:

```python
import pytest
from assay import Coverage

def test_agent_compliance():
    """
    Verify that the agent's recent run complies with the policy
    and achieves sufficient tool coverage.
    """
    trace_file = "traces/latest_run.jsonl"

    # Initialize coverage analyzer with your policy
    cov = Coverage("assay.yaml")

    # Analyze traces
    report = cov.analyze(trace_file, min_coverage=90.0)

    # Assert compliance
    assert report.meets_threshold, \
        f"Coverage failed! Got {report.overall_coverage_pct}%, needed 90%"

    # Check for specific high-risk gaps
    assert not report.high_risk_gaps, \
        "Found high-risk tools that were never called: " + str(report.high_risk_gaps)
```

## Using the Pytest Fixture

Assay includes a `pytest` fixture for convenience if installed via `pip install assay`.

```python
def test_with_fixture(assay_client):
    # Record a live trace (if running a live agent test)
    assay_client.record_trace(
        tool="search_kb",
        args={"query": "payment failure"}
    )

    # Or validate existing files
    # ...
```

## CI Integration

Run your tests as part of your standard CI pipeline:

```yaml
# .github/workflows/test.yml
steps:
  - uses: actions/checkout@v4
  - name: Install dependencies
    run: pip install pytest assay

  - name: Run compliance tests
    run: pytest test_agent.py
```
