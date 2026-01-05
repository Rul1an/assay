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

The fastest way to validate an MCP trace against a policy (no database required):

```bash
# 1. Install CLI
curl -sSL https://assay.dev/install.sh | sh

# 2. Validate a trace file
assay validate --config assay.yaml --trace-file traces.jsonl
```

For advanced features like **historical regression testing** and **CI gates**, see the [Usage](#cli-usage) section below.

## Installation

### Python SDK
```bash
pip install assay
```

### CLI (Linux/macOS)
```bash
curl -sSL https://assay.dev/install.sh | sh
```

### GitHub Action
```yaml
# .github/workflows/ci.yml
- uses: assay-dev/assay-action@v1
  with:
    policy: policies/agent.yaml
    traces: traces/
```

## Quick Start (Python)

### 1. Define Policy (assay.yaml)

```yaml
version: 1
tools:
  deploy_prod:
    args:
      properties:
        force: { const: false } # Block force=true
        cluster: { pattern: "^(eu|us)-west-[0-9]$" }
    sequence:
      before: ["check_health"] # Must check health before deploy
```

### 2. Validate Traces

```python
# test_compliance.py
import json
import pytest
from assay import Coverage

def test_tool_coverage():
    # Load traces
    with open("traces/session.jsonl") as f:
        traces = [json.loads(line) for line in f]

    # Enforce policy
    cov = Coverage("assay.yaml")
    report = cov.analyze(traces, min_coverage=80.0)

    assert report["meets_threshold"], \
        f"Coverage too low: {report['overall_coverage_pct']}%"
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
