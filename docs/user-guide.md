# User Guide

Assay ensures your Agentic System is **production-ready** by enforcing strict policies on tool usage.

## 🚀 Workflows

### 1. The CI/CD Gate (Recommended)
This workflow ensures no broken agent code merges to `main`.

1.  **Init**: Run `assay init-ci` to generate a GitHub Actions or GitLab CI workflow.
2.  **Commit**: Push your `assay.yaml` policy and your `traces/` (golden dataset).
3.  **Gate**: On every PR, Assay verifies your agent's current traces against the policy.

### 2. The Local Clinic (`doctor`)
Use `assay doctor` when things go wrong.

```bash
$ assay doctor
Diagnosing... Note: Found 1 issue.
[ERROR] Policy 'deploy' requires 'env' arg, but trace missing it.
[HINT]  Did you mean 'environment'?
```

### 3. Python Tests (`pytest`)
For developers who prefer defining tests in code.

```python
from assay import validate

def test_agent_logic(traces):
    assert validate("assay.yaml", traces)["passed"]
```

## 🧠 Core Concepts

### Policy-as-Code
Assay does **not** use LLMs to evaluate your agent. It uses **Logic**.
If you define `replicas < 5`, and the agent calls with `replicas: 10`, it fails. 100% of the time.

### Statelessness
Validation requires only two inputs:
1.  **Policy File** (`assay.yaml`)
2.  **Trace List** (JSONL or List of Dicts)

This means you can run Assay **anywhere**: Local, CI, Docker, Airgapped.

### Determinism
Unlike "LLM-as-a-Judge" evaluators, Assay's output is deterministic.
-   Same Input + Same Policy = Same Result.
-   Zero flakiness.

## 🛠 Advanced Features

### Baseline Regression
Ensure your agent doesn't get *worse*.

1.  **Export Baseline**: `assay ci --export-baseline baseline.json` (on `main`).
2.  **Compare**: `assay ci --baseline baseline.json` (on `feat-branch`).

If coverage drops by >5% (configurable), the build fails.

### Friendly Hints
Assay's error messages are designed for humans.
-   **Fuzzy Matching**: Detects typos in tool names.
-   **Context**: Shows lines of code where the error occurred (in Python SDK).

## 📚 Reference

-   [**CLI Commands**](cli/index.md)
-   [**Configuration Schema**](config/index.md)
