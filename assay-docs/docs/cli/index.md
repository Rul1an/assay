# CLI Reference

Complete documentation for all Assay commands.

---

## Installation

```bash
# Python
pip install assay

# Rust
cargo install assay
```

Verify installation:

```bash
assay --version
# assay 0.8.0
```

---

## Commands Overview

| Command | Description |
|---------|-------------|
| [`assay run`](run.md) | Run tests against traces |
| [`assay import`](import.md) | Import sessions from MCP Inspector, etc. |
| [`assay migrate`](migrate.md) | Upgrade config from v0 to v1 |
| [`assay replay`](replay.md) | Interactive trace replay |
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

### Import Traces

```bash
# From MCP Inspector
assay import --format mcp-inspector session.json

# Auto-generate config
assay import --format mcp-inspector session.json --init

# Custom output path
assay import --format mcp-inspector session.json --out-trace traces/custom.jsonl
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

    Interactive step-by-step trace replay for debugging.

    [:octicons-arrow-right-24: Full reference](replay.md)

-   :material-server:{ .lg .middle } __assay mcp-server__

    ---

    Start Assay as an MCP tool server for agent self-correction.

    [:octicons-arrow-right-24: Full reference](mcp-server.md)

</div>
