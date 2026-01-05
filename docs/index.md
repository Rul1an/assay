<h1 align="center">
  <br>
  <img src="assets/logo.svg" alt="Assay Logo" width="200">
  <br>
  Assay
  <br>
</h1>

<p class="subtitle">The CI/CD Standard for Agentic Systems</p>

Assay is a strict **Policy-as-Code** engine for Model Context Protocol (MCP). It validates that your AI Agents use tools correctly, enforcing schema limits and sequence rules before they hit production.

---

## What is Assay?

<div class="grid cards" markdown>

-   :material-robot:{ .lg .middle } __For Vibecoders__

    ---

    You build agents with natural language. Assay is your **Guardrail**. Connect your traces, run `assay validate`, and see if your agent is trying to delete the production database.

    [:octicons-arrow-right-24: Python Quickstart](python-quickstart.md)

-   :material-console:{ .lg .middle } __For Engineers__

    ---

    You need **Determinism**. Assay is a high-performance Rust binary that enforces rigid JSON Schemas and sequence constraints in CI. No flaky evals.

    [:octicons-arrow-right-24: CLI Reference](cli/index.md)

</div>

## How it Works

### 1. Define Policy
Create an `assay.yaml` that defines valid tool usage.

```yaml
version: 1
tools:
  deploy_service:
    args:
      properties:
        env: { pattern: "^(staging|prod)$" } # Enforce regex
    sequence:
      before: ["run_tests"] # Must run tests before deploy
```

### 2. Capture Traces
Log your agent's MCP tool calls to a JSONL file.

```json
{"tool": "run_tests", "args": {}}
{"tool": "deploy_service", "args": {"env": "prod"}}
```

### 3. Validate
Run the validation engine (Stateless).

```bash
assay validate --config assay.yaml --trace-file traces.jsonl
```

| Result | Status | Reason |
| :--- | :--- | :--- |
| **Pass** | ✅ | Schema matches, Sequence respected. |
| **Fail** | ❌ | `env` was "dev" (pattern mismatch). |

## Key Features

- **Stateless**: No database required. Validate in GitHub Actions, GitLab CI, or local `pytest`.
- **The Doctor**: `assay doctor` automatically diagnoses config errors and fixes typos.
- **CI-Native**: `assay init-ci` generates workflow files for you.
- **Fast**: Written in Rust. <10ms overhead.

## Next Steps

- [**Get Started**](getting-started/index.md)
- [**Python SDK**](python-quickstart.md)
- [**Config Reference**](config/index.md)
