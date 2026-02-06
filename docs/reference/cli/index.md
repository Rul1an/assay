# CLI Reference

Complete documentation for all Assay commands.

---

## Installation

```bash
# Rust
cargo install assay-cli
# Or via installer scripts (see Home)
```

Verify installation:

```bash
assay --version
# assay 0.9.0
```

---

## Commands Overview

| Command | Description |
|---------|-------------|
| [`assay run`](run.md) | Run tests against traces |
| [`assay explain`](explain.md) | Explain why trace steps were allowed/blocked |
| [`assay bundle`](bundle.md) | Create/verify replay bundles |
| [`assay replay`](replay.md) | Replay from a replay bundle |
| [`assay import`](import.md) | Import sessions from MCP Inspector, etc. |
| [`assay migrate`](migrate.md) | Upgrade config from v0 to v1 |
| [`assay doctor`](doctor.md) | Diagnose setup and optionally auto-fix known issues |
| [`assay watch`](watch.md) | Re-run on config/policy/trace changes |
| [`assay monitor`](../../guides/runtime-monitor.md) | **Runtime Security** (Linux Kernel Enforcement) |
| [`assay mcp wrap`](mcp-server.md) | Wrap an MCP process with policy enforcement |

---

## Global Options

Common top-level options:

| Option | Description |
|--------|-------------|
| `--help`, `-h` | Show help message |
| `--version`, `-V` | Show version |

---

## Quick Examples

### Run Tests

```bash
# Basic run
assay run --config eval.yaml

# Strict mode (fail on any violation)
assay run --config eval.yaml --strict

# Specific trace file
assay run --config eval.yaml --trace-file traces/golden.jsonl

# CI reports
assay ci --config eval.yaml --trace-file traces/golden.jsonl --sarif sarif.json --junit junit.xml
```

### Replay Bundles

```bash
# Create bundle from latest run artifacts
assay bundle create

# Verify bundle safety/integrity
assay bundle verify --bundle .assay/bundles/12345.tar.gz

# Replay from bundle (offline default)
assay replay --bundle .assay/bundles/12345.tar.gz

# Replay live with seed override
assay replay --bundle .assay/bundles/12345.tar.gz --live --seed 42
```

### Migrate Config

```bash
# Upgrade to v1 format
assay migrate --config old-eval.yaml

# Preview changes without writing
assay migrate --config old-eval.yaml --dry-run
```

### Start MCP Wrapper

```bash
# Enforcing mode
assay mcp wrap --policy assay.yaml -- <real-mcp-command> [args...]

# Dry-run mode
assay mcp wrap --policy assay.yaml --dry-run -- <real-mcp-command> [args...]
```

### Diagnose and Watch

```bash
# Diagnose and auto-fix known issues
assay doctor --config eval.yaml --trace-file traces/dev.jsonl --fix --yes

# Live re-run loop on local edits
assay watch --config eval.yaml --trace-file traces/dev.jsonl --strict
```

---

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success (all tests passed) |
| 1 | Test failure (one or more tests failed) |
| 2 | Configuration error |
| 3 | Infrastructure/judge error |
| 4 | Would block (sandbox/policy) |

---

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `ASSAY_EXIT_CODES` | Exit code compatibility mode (`v1` or `v2`) | `v2` |
| `MCP_CONFIG_LEGACY` | Enable legacy config mode when set to `1` | disabled |
| `ASSAY_STRICT_DEPRECATIONS` | Fail on deprecated policy/config usage when set to `1` | disabled |
| `OPENAI_API_KEY` | API key for OpenAI-backed judge/embedder paths | unset |
| `NO_COLOR` | Disable colored output | unset |

---

## Configuration File

Most run/ci commands read from `eval.yaml` by default:

```yaml
version: 1
suite: my-agent
model: gpt-4o-mini
tests:
  - id: args_valid
    input:
      prompt: "Summarize this task."
    expected:
      type: args_valid
      policy: policies/default.yaml
```

See [Configuration](../config/index.md) for full reference.

---

## Command Details

<div class="grid cards" markdown>

-   :material-play:{ .lg .middle } __assay run__

    ---

    Run tests against traces. The main command for CI/CD.

    [:octicons-arrow-right-24: Full reference](run.md)

-   :material-file-search-outline:{ .lg .middle } __assay explain__

    ---

    Explain blocked/allowed trace steps and evaluated rules.

    [:octicons-arrow-right-24: Full reference](explain.md)

-   :material-import:{ .lg .middle } __assay import__

    ---

    Import sessions from MCP Inspector and other formats.

    [:octicons-arrow-right-24: Full reference](import.md)

-   :material-update:{ .lg .middle } __assay migrate__

    ---

    Upgrade configuration from v0 to v1 format.

    [:octicons-arrow-right-24: Full reference](migrate.md)

-   :material-stethoscope:{ .lg .middle } __assay doctor__

    ---

    Diagnose environment/config issues and apply known fixes.

    [:octicons-arrow-right-24: Full reference](doctor.md)

-   :material-eye-refresh:{ .lg .middle } __assay watch__

    ---

    Watch files and rerun Assay on changes.

    [:octicons-arrow-right-24: Full reference](watch.md)

-   :material-step-forward:{ .lg .middle } __assay replay__

    ---

    Replay runs from a replay bundle (`--bundle`), offline by default.

    [:octicons-arrow-right-24: Full reference](replay.md)

-   :material-package-variant:{ .lg .middle } __assay bundle__

    ---

    Create and verify replay bundles.

    [:octicons-arrow-right-24: Full reference](bundle.md)

-   :material-server:{ .lg .middle } __assay mcp wrap__

    ---

    Wrap a real MCP process with policy enforcement for agent self-correction.

    [:octicons-arrow-right-24: Full reference](mcp-server.md)

-   :material-shield-lock:{ .lg .middle } __assay monitor__

    ---

    Real-time kernel enforcement (SOTA). Blocks attacks before they happen.

    [:octicons-arrow-right-24: Runtime Reference](../runtime-monitor.md)

</div>
