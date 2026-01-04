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

Assay is a toolchain for validating **Model Context Protocol (MCP)** interactions. It enforces strict schema policies and sequence constraints on JSON-RPC `call_tool` payloads.

**Use Cases:**
*   **CI/CD**: Deterministic replay of tool execution traces to prevent regressions.
*   **Runtime Gate**: Sidecar proxy to block non-compliant tool calls before they reach production services.
*   **Compliance**: Audit log validation against defined policy files (`allow/block` lists, arg validation).

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

## Quick Start

### 1. Validate with Python (Recommended)
Write a native Pytest to validate tool coverage and policy compliance:

```python
# test_compliance.py
import pytest
from assay import Coverage

def test_tool_coverage():
    # Load traces from your agent run
    traces = "traces/session.jsonl"

    # Enforce policy coverage
    cov = Coverage("assay.yaml")
    report = cov.analyze(traces, min_coverage=80.0)

    assert report.meets_threshold, f"Coverage too low: {report.overall_coverage_pct}%"
```

### 2. Define Policy (assay.yaml)
Define allowed tools and constraints:

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

### 2. Validate Traces (CI)
Run against captured Inspector or OTel logs:

```bash
assay run --config assay.yaml --trace-file traces/session.jsonl --strict
```

### 3. Run Policy Server
Start an MCP-compliant server to validate calls in real-time:

```bash
assay mcp-server --port 3001 --policy .
```

The server exposes `assay_check_args` and `assay_check_sequence` as MCP tools, allowing agents to self-correct or be blocked by a supervisor.

## Documentation

Full reference: [docs.assay.dev](https://docs.assay.dev)

*   [Configuration Schema](https://docs.assay.dev/config/)
*   [CLI Commands](https://docs.assay.dev/cli/)
*   [MCP Protocol Integration](https://docs.assay.dev/mcp/)

## License

MIT.
