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
| [`assay monitor`](../../guides/runtime-monitor.md) | **Runtime Security** (Linux Kernel Enforcement) |
| [`assay mcp-server`](mcp-server.md) | Start Assay as MCP tool server |

---

## Global Options

These options work with all commands:

| Option | Description |
|--------|-------------|
| `--help`, `-h` | Show help message |
| `--version`, `-V` | Show version |
| `--verbose`, `-v` | Enable verbose output |
| `--quiet`, `-q` | Suppress non-error output |
| `--config`, `-c` | Path to mcp-eval.yaml |

---

## Quick Examples

### Run Tests

```bash
# Basic run
assay run --config mcp-eval.yaml

# Strict mode (fail on any violation)
assay run --config mcp-eval.yaml --strict

# Specific trace file
assay run --config mcp-eval.yaml --trace-file traces/golden.jsonl

# Output formats
assay run --config mcp-eval.yaml --output sarif
assay run --config mcp-eval.yaml --output junit
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

### Start MCP Server

```bash
# Default port
assay mcp-server --policy policies/

# Custom port
assay mcp-server --port 3001 --policy policies/
```

---

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success (all tests passed) |
| 1 | Test failure (one or more tests failed) |
| 2 | Configuration error |
| 3 | File not found |
| 4 | Invalid input format |

---

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `ASSAY_CONFIG` | Default config file path | `mcp-eval.yaml` |
| `ASSAY_DB` | Database path | `.assay/store.db` |
| `ASSAY_LOG_LEVEL` | Log verbosity | `info` |
| `NO_COLOR` | Disable colored output | unset |

---

## Configuration File

Most commands read from `mcp-eval.yaml`:

```yaml
version: "1"
suite: my-agent

tests:
  - id: args_valid
    metric: args_valid
    policy: policies/default.yaml

output:
  format: [sarif, junit]
  directory: .assay/reports
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

-   :material-step-forward:{ .lg .middle } __assay replay__

    ---

    Replay runs from a replay bundle (`--bundle`), offline by default.

    [:octicons-arrow-right-24: Full reference](replay.md)

-   :material-package-variant:{ .lg .middle } __assay bundle__

    ---

    Create and verify replay bundles.

    [:octicons-arrow-right-24: Full reference](bundle.md)

-   :material-server:{ .lg .middle } __assay mcp-server__

    ---

    Start Assay as an MCP tool server for agent self-correction.

    [:octicons-arrow-right-24: Full reference](mcp-server.md)

-   :material-shield-lock:{ .lg .middle } __assay monitor__

    ---

    Real-time kernel enforcement (SOTA). Blocks attacks before they happen.

    [:octicons-arrow-right-24: Runtime Reference](../runtime-monitor.md)

</div>
