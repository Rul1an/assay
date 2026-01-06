<h1 align="center">
  <br>
  <img src="docs/assets/logo.svg" alt="Assay Logo" width="120">
  <br>
  Assay
  <br>
</h1>

<h4 align="center">Runtime Security for the Agentic Web</h4>

<p align="center">
  <a href="https://github.com/Rul1an/assay/actions/workflows/ci.yml">
    <img src="https://github.com/Rul1an/assay/actions/workflows/ci.yml/badge.svg" alt="CI Status">
  </a>
  <a href="https://crates.io/crates/assay-core">
    <img src="https://img.shields.io/crates/v/assay-core.svg?color=10b981" alt="Crates.io">
  </a>
  <a href="https://github.com/Rul1an/assay#readme">
    <img src="https://img.shields.io/badge/docs-github-blue" alt="Documentation">
  </a>
</p>

---

**Assay** is a high-performance policy engine for AI Agents. It sits between your LLM and your MCP servers, enforcing strict **Policy-as-Code** to prevent unauthorized tool access, argument injection, and hallucinations.

**For Engineers**: Rust-powered, sub-millisecond latency, strictly typed.
**For Agents**: Deterministic environment, actionable error messages, self-healing config.

## 🚀 Features

-   **MCP Firewall**: Wraps any MCP server to enforce allowlists, argument regex, and rate limits.
-   **Policy-as-Code**: Define security rules in simple, version-controlled YAML.
-   **The Doctor**: Self-repairing CLI (`assay doctor`) that fixes drift and config errors automatically.
-   **CI-Native**: Generates GitHub/GitLab workflows instantly (`assay init-ci`).
-   **Python SDK**: Stateless validation for `pytest` and localized evaluation.

## ⚡ Quick Start

### 1. Install (macOS / Linux / WSL)

```bash
curl -fsSL https://getassay.dev/install.sh | sh
```

### 2. Protect an MCP Server

Wrap your existing MCP server command with `assay mcp` to inject the security layer.

```bash
# Before:
uvx project-mcp-server

# After (Protected):
assay mcp run --policy policy.yaml -- uvx project-mcp-server
```

### 3. Generate Config for Claude/Cursor

Stop fighting JSON manually. Let Assay discover your local config and generate secure snippets.

```bash
assay mcp config-path
```

### 4. CI/CD Pipeline

```bash
# Generate a ready-to-merge GitHub Actions workflow
assay init-ci --provider github
```

## 🐍 Python SDK

Integrate strictly typed validation into your `pytest` suite.

```bash
pip install assay-it
```

```python
from assay import validate

def test_agent_compliance(traces):
    """
    Validate agent traces against your defined policy.
    Raises strict errors on violations.
    """
    report = validate(
        policy_path="assay.yaml",
        traces=traces
    )

    assert report["passed"], f"Policy Violation: {report['violations']}"
```

## 🛠️ Components

| Crate | Description |
| :--- | :--- |
| **`assay-core`** | The policy engine kernel. Zero dependencies, pure Rust. |
| **`assay-cli`** | The developer experience. Logic, Doctor, and Init workflows. |
| **`assay-mcp-server`** | The MITM proxy that secures MCP connections. |
| **`assay-metrics`** | Telemetry and structured logging events. |
| **`assay-python-sdk`** | PyO3 bindings for Python integrations. |

## 📚 Documentation

Detailed guides and references are available in the [**docs/**](docs/) directory or on GitHub.

-   [**Getting Started**](docs/getting-started.md)
-   [**Configuration Schema**](docs/config.md)
-   [**Python Reference**](docs/python-sdk.md)

## License

MIT.
