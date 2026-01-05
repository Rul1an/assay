<h1 align="center">
  <br>
  <img src="docs/assets/logo.svg" alt="Assay Logo" width="200">
  <br>
  Assay
  <br>
</h1>

<h4 align="center">MCP Integration Testing & Policy Engine</h4>

<p align="center">
  <a href="https://github.com/Rul1an/assay/actions/workflows/assay.yml">
    <img src="https://github.com/Rul1an/assay/actions/workflows/assay.yml/badge.svg" alt="CI Status">
  </a>
  <a href="https://crates.io/crates/assay">
    <img src="https://img.shields.io/crates/v/assay.svg" alt="Crates.io">
  </a>
  <a href="https://docs.assay.dev">
    <img src="https://img.shields.io/badge/docs-assay.dev-blue" alt="Documentation">
  </a>
</p>

---

## Overview

Assay validates **Model Context Protocol (MCP)** interactions. It enforces schema policies and sequence constraints on JSON-RPC `call_tool` payloads.

**Use Cases:**
*   **CI/CD**: Deterministic replay of tool execution traces.
*   **Runtime Gate**: Proxy to block non-compliant tool calls.
*   **Compliance**: Audit log validation against policy files.

## Quick Start

The fastest way to get started is the interactive demo:

```bash
# 1. Install CLI
curl -sSL https://assay.dev/install.sh | sh

# 2. Run the instant demo (Generates valid policy & traces)
assay demo

# 3. See it pass!
# ✅ Validation Passed!
```

### Stateless Validation

You don't need a database. Validates traces against a policy file directly:

```bash
assay validate --config assay.yaml --trace-file traces.jsonl
```

### Python SDK

Validate traces in your Python tests with a clean, stateless API:

```python
from assay import validate

# Returns a report dict (raises if config is invalid)
report = validate(policy="assay.yaml", traces=my_traces)
assert report["success"]
```

## Installation

### CLI
```bash
curl -sSL https://assay.dev/install.sh | sh
```

### Python
```bash
pip install assay
```

### GitHub Action
```yaml
# .github/workflows/ci.yml
- uses: assay-dev/assay-action@v1.2
  with:
    command: validate
    config: assay.yaml
    trace-file: traces.jsonl
```

### Python Validation Example

```python
import json
import pytest
from assay import validate

def test_tool_compliance():
    # 1. Load traces (or capture them)
    with open("traces.jsonl") as f:
        traces = [json.loads(line) for line in f]

    # 2. Validate against policy (Stateless)
    report = validate(policy="assay.yaml", traces=traces)

    assert report["meets_threshold"], f"Policy violation: {report['violations']}"
```

## CLI Usage

Validate captured Inspector or OTel logs:

```bash
assay run --config assay.yaml --trace-file traces/session.jsonl --strict
```

Start an MCP-compliant policy server:

```bash
assay mcp-server --port 3001 --policy .
```

## Documentation

Full reference: [docs.assay.dev](https://docs.assay.dev)

*   [Configuration Schema](https://docs.assay.dev/config/)
*   [CLI Commands](https://docs.assay.dev/cli/)
*   [MCP Protocol Integration](https://docs.assay.dev/mcp/)

## License

MIT.
