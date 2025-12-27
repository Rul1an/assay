# CLI Reference

Complete reference for all Assay commands.

## Commands Overview

| Command | Description |
|---------|-------------|
| `assay run` | Execute tests against a trace file |
| `assay import` | Convert logs to Assay trace format |
| `assay migrate` | Upgrade legacy v0 configs to v1 |

---

## assay run

Execute tests from a config file against a trace.

### Usage

```bash
assay run --config <CONFIG> --trace-file <TRACE> [OPTIONS]
```

### Required Arguments

| Argument | Description |
|----------|-------------|
| `--config <PATH>` | Path to `mcp-eval.yaml` config file |
| `--trace-file <PATH>` | Path to `.jsonl` trace file |

### Options

| Option | Default | Description |
|--------|---------|-------------|
| `--strict` | `false` | Exit with code 1 on any failure (for CI) |
| `--db <PATH>` | `.assay/store.db` | SQLite database path |
| `--db :memory:` | - | Use in-memory database (ephemeral) |
| `--parallel <N>` | `4` | Number of parallel test workers |
| `--timeout <SECONDS>` | `10` | Per-test timeout |

### Output Examples

**Pass:**
```
Running 1 tests...
✅ test_golden_1        passed (0.2s)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Summary: 1 passed, 0 failed, 0 skipped
```

**Fail:**
```
Running 1 tests...
❌ test_golden_1        failed: sequence_valid  (0.0s)
      Prompt: "calls tool"
      Message: Missing required tool: missing_tool
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Summary: 0 passed, 1 failed, 0 skipped
```

**Multiple tests:**
```
Running 5 tests...
✅ deploy_schema_check          passed (0.1s)
✅ database_migration_flow      passed (0.2s)
❌ injection_attempt            failed: tool_blocklist  (0.0s)
      Message: Blocked tool called: delete_users
✅ output_formatting            passed (0.1s)
⏭️  cached_test                 skipped (fingerprint match)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Summary: 3 passed, 1 failed, 1 skipped
```

### Exit Codes

| Code | Meaning |
|------|---------|
| `0` | All tests passed (or only skipped) |
| `1` | One or more tests failed |
| `2` | Configuration error |

---

## assay import

Convert external logs to Assay trace format.

### Usage

```bash
assay import --format <FORMAT> <INPUT> --out-trace <OUTPUT> [OPTIONS]
```

### Required Arguments

| Argument | Description |
|----------|-------------|
| `--format <FORMAT>` | Input format: `mcp-inspector`, `json-rpc` |
| `<INPUT>` | Path to input log file |
| `--out-trace <PATH>` | Output trace file path (`.jsonl`) |

### Options

| Option | Description |
|--------|-------------|
| `--init` | Generate a starter `mcp-eval.yaml` from the trace |
| `--update` | Update existing trace (merge new events) |

### Examples

**Basic import:**
```bash
assay import --format mcp-inspector session.json --out-trace trace.jsonl
```

**Generate starter config:**
```bash
assay import --format mcp-inspector good_run.json --out-trace golden.jsonl --init
# Creates mcp-eval.yaml with inferred policies
```

### Output

```
Imported 42 events from session.json
Written to: trace.jsonl

Detected tools:
  - deploy_service (called 3 times)
  - check_status (called 2 times)
  - notify_slack (called 1 time)
```

---

## assay migrate

Upgrade legacy v0 configs to the v1 format.

### Usage

```bash
assay migrate --config <CONFIG>
```

### What It Does

1. Creates a backup (`config.yaml.bak`)
2. Reads external policy files from `policies/*.yaml`
3. Inlines all policies into the main config
4. Adds `configVersion: 1` header
5. Converts legacy sequence syntax to DSL

### Example

**Before (v0):**
```yaml
# eval.yaml (v0 - legacy)
suite: my_agent
tests:
  - id: test1
    policies:
      - $ref: policies/args.yaml
      - $ref: policies/sequence.yaml
```

```yaml
# policies/args.yaml
type: args_valid
schema:
  deploy_service:
    type: object
```

**After running `assay migrate --config eval.yaml`:**
```yaml
# eval.yaml (v1 - migrated)
configVersion: 1
suite: my_agent
tests:
  - id: test1
    expected:
      type: args_valid
      schema:
        deploy_service:
          type: object
```

### Output

```
Migrating eval.yaml...
  Created backup: eval.yaml.bak
  Inlined 2 policy files:
    - policies/args.yaml
    - policies/sequence.yaml
  Upgraded to configVersion: 1
Done.
```

---

## Common Workflows

### Local Development

```bash
# Run tests with verbose output
assay run --config mcp-eval.yaml --trace-file trace.jsonl

# Use in-memory DB for isolation
assay run --config mcp-eval.yaml --trace-file trace.jsonl --db :memory:
```

### CI/CD Pipeline

```bash
# Strict mode: fail on any test failure
assay run --config mcp-eval.yaml --trace-file goldens.jsonl --strict

# Echo exit code for debugging
assay run --config mcp-eval.yaml --trace-file goldens.jsonl --strict || echo "Exit: $?"
```

### Debugging a Failure

```bash
# Step 1: Import the problematic trace
assay import --format mcp-inspector bug_report.json --out-trace bug.jsonl

# Step 2: Run against your policies
assay run --config mcp-eval.yaml --trace-file bug.jsonl

# Step 3: See which policy failed and fix it
```

### Creating a New Test Suite

```bash
# Step 1: Record a "golden" session
# (Use MCP Inspector or your agent's logging)

# Step 2: Import and generate starter config
assay import --format mcp-inspector golden_session.json --out-trace golden.jsonl --init

# Step 3: Review and tighten the generated policies
vim mcp-eval.yaml

# Step 4: Verify it passes
assay run --config mcp-eval.yaml --trace-file golden.jsonl
```

---

## Environment Variables

| Variable | Description |
|----------|-------------|
| `ASSAY_CONFIG` | Default config path (instead of `--config`) |
| `ASSAY_DB` | Default database path (instead of `--db`) |
| `RUST_LOG=assay=debug` | Enable debug logging |

---

## Database Paths

| Path | Use Case |
|------|----------|
| `.assay/store.db` | Default, project-local |
| `:memory:` | Ephemeral, no persistence (CI) |
| `/tmp/assay.db` | Temporary, cross-run persistence |
| `~/.assay/global.db` | Shared across projects |
