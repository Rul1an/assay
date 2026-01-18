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

-   :material-flash:{ .lg .middle } __Install Now__

    ---

    Get the binary in seconds via our new installer.

    [:octicons-arrow-right-24: getassay.dev](https://getassay.dev)

-   :material-robot:{ .lg .middle } __For Agent Developers__

    ---

    You build agents with natural language. Assay is your **Guardrail**. Connect your traces, run `assay validate`, or use `assay monitor` to block attacks at the kernel level.

    [:octicons-arrow-right-24: Getting Started](getting-started/index.md)

-   :material-console:{ .lg .middle } __For Engineers__

    ---

    You need **Determinism**. Assay is a high-performance Rust binary that enforces rigid JSON Schemas and sequence constraints in CI. No flaky evals.

    [:octicons-arrow-right-24: CLI Reference](cli/index.md)

</div>

## How it Works

### 1. Initialize
Run the wizard to auto-detect your project type and generate secure defaults.

```bash
assay init
```

### 2. Capture Traces
Log your agent's MCP tool calls to a JSONL file.

```json
{"tool": "run_tests", "args": {}}
{"tool": "deploy_service", "args": {"env": "prod"}}
```

### 3. Validate
Run the validation engine (Stateless). Supports **SARIF** for GitHub Advanced Security.

```bash
assay validate --trace-file traces.jsonl --format sarif
```

### 4. Enable Runtime Security (Linux)
Block IO and Network access at the kernel level.

```bash
sudo assay monitor --policy policy.yaml
```

| Result | Status | Output |
| :--- | :--- | :--- |
| **Pass** | ✅ | `exit code 0` |
| **Fail** | ❌ | `exit code 1` + SARIF report |
| **Error** | ⚠️ | `exit code 2` (Config/Schema validation) |

## Key Features

- **Stateless**: No database required. Validate in GitHub Actions, GitLab CI, or local `pytest`.
- **The Doctor**: `assay doctor` automatically diagnoses config errors.
- **Agentic Contract**: JSON output optimized for AI agents (`--format json`).
- **CI-Native**: `assay init --ci` generates GitHub Actions workflows.
- **Fast**: Written in Rust. <10ms overhead.

## Next Steps

- [**Get Started**](getting-started/index.md)
- [**Python SDK**](python-quickstart.md)
- [**Config Reference**](config/index.md)
