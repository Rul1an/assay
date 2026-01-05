<h1 align="center">
  <br>
  <img src="docs/assets/logo.svg" alt="Assay Logo" width="200">
  <br>
  Assay
  <br>
</h1>

<h4 align="center">The CI/CD Standard for Agentic Systems</h4>

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

**Assay** is the missing link between your AI Agent and Production. It enforces **Policy-as-Code** on Model Context Protocol (MCP) tool usage, ensuring your agents behave safely and deterministically.

**For Vibecoders**: Connect your agent output, run `assay validate`, and see if it breaks the rules.
**For Engineers**: High-performance Rust binary, rigid schema validation, and CI/CD integration.

## 🚀 Features

-   **Policy Engine**: Enforce schema strictness and sequence ordering (`search_before_escalate`).
-   **The Doctor**: Smart diagnostics (`assay doctor`) that fix typo'd tool names and config issues automatically.
-   **CI-Native**: Generates GitHub/GitLab workflows instantly (`assay init-ci`).
-   **Stateless Python SDK**: Validate traces directly in `pytest` without a database.

## ⚡ Quick Start

### 1. Instant Verification
Don't guess. Verify.

```bash
# Install (macOS/Linux)
curl -sSL https://assay.dev/install.sh | sh

# Generate a demo environment (policy + traces) and run checks
assay demo
```

### 2. CI/CD in 10 Seconds
Stop writing YAML manually.

```bash
# Generate a ready-to-merge GitHub Actions workflow
assay init-ci --provider github
```

### 3. The Clinic (Debugging)
Something wrong? Let the Doctor diagnose it.

```bash
# Analyzes your config, policies, and traces for common issues
assay doctor
```

## 🐍 Python SDK

Integrate directly into your `pytest` suite. No server required.

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

> **Note**: Full docstrings and type hints included. Your IDE will love you.

## 🛠️ CLI Reference

| Command | Purpose |
| :--- | :--- |
| `assay validate` | Run stateless validation on trace files. |
| `assay doctor` | Diagnose config issues and fuzzy-match known errors. |
| `assay init-ci` | Generate CI workflow templates (GitHub/GitLab). |
| `assay run` | Execute and capture traces from an agent (Advanced). |

## 📚 Documentation

-   [**Getting Started**](https://docs.assay.dev/getting-started/)
-   [**Python Quickstart**](https://docs.assay.dev/python-quickstart/)
-   [**Configuration Schema**](https://docs.assay.dev/config/)

## License

MIT.
