# assay run

Run tests against traces. The primary command for CI/CD pipelines.

---

## Synopsis

```bash
assay run [OPTIONS]
```

---

## Description

Runs all tests defined in your configuration file against the specified trace(s). This is the main command for:

- CI/CD regression gates
- Local development testing
- Baseline comparison

---

## Options

### Required

| Option | Description |
|--------|-------------|
| `--config`, `-c` | Path to mcp-eval.yaml (default: `mcp-eval.yaml`) |

### Trace Selection

| Option | Description |
|--------|-------------|
| `--trace-file`, `-t` | Path to specific trace file |
| `--trace-dir` | Directory containing trace files (runs all) |

### Execution Mode

| Option | Description |
|--------|-------------|
| `--strict` | Fail on any violation (default for CI) |
| `--lenient` | Report violations but don't fail |
| `--no-cache` | Skip cache, always re-run tests |
| `--db` | Database path (use `:memory:` for in-memory) |

### Output

| Option | Description |
|--------|-------------|
| `--output`, `-o` | Output format: `sarif`, `junit`, `json`, `text` |
| `--output-dir` | Directory for output files |
| `--output-log` | Write detailed log to file |
| `--verbose`, `-v` | Enable verbose output |
| `--quiet`, `-q` | Suppress non-error output |

### Filtering

| Option | Description |
|--------|-------------|
| `--test` | Run only specific test(s) by ID |
| `--metric` | Run only specific metric(s) |
| `--tool` | Validate only specific tool(s) |

---

## Examples

### Basic Usage

```bash
# Run all tests
assay run --config mcp-eval.yaml

# Run with specific trace
assay run --config mcp-eval.yaml --trace-file traces/golden.jsonl
```

### CI Mode

```bash
# Strict mode with SARIF output
assay run \
  --config mcp-eval.yaml \
  --strict \
  --output sarif \
  --db :memory:
```

### Development Mode

```bash
# Verbose output for debugging
assay run --config mcp-eval.yaml --verbose

# Lenient mode for exploration
assay run --config mcp-eval.yaml --lenient
```

### Filtered Runs

```bash
# Run only specific tests
assay run --config mcp-eval.yaml --test args_valid --test sequence_check

# Validate only specific tools
assay run --config mcp-eval.yaml --tool apply_discount --tool process_payment
```

### Multiple Traces

```bash
# Run against all traces in directory
assay run --config mcp-eval.yaml --trace-dir traces/

# Run against multiple specific traces
assay run --config mcp-eval.yaml \
  --trace-file traces/happy-path.jsonl \
  --trace-file traces/edge-case.jsonl
```

---

## Output Formats

### SARIF (GitHub Code Scanning)

```bash
assay run --config mcp-eval.yaml --output sarif
# Creates: .assay/reports/results.sarif
```

Upload to GitHub:

```yaml
- uses: github/codeql-action/upload-sarif@v2
  with:
    sarif_file: .assay/reports/results.sarif
```

### JUnit (CI Test Results)

```bash
assay run --config mcp-eval.yaml --output junit
# Creates: .assay/reports/junit.xml
```

### JSON (Programmatic)

```bash
assay run --config mcp-eval.yaml --output json
# Creates: .assay/reports/results.json
```

### Text (Human-Readable)

```bash
assay run --config mcp-eval.yaml --output text

# Output:
# Assay v0.8.0 — Zero-Flake CI for AI Agents
#
# ┌───────────────────┬────────┬─────────────────────────┐
# │ Test              │ Status │ Details                 │
# ├───────────────────┼────────┼─────────────────────────┤
# │ args_valid        │ ✅ PASS │ 47/47 calls valid       │
# │ sequence_valid    │ ✅ PASS │ All rules satisfied     │
# │ tool_blocklist    │ ✅ PASS │ No blocked tools called │
# └───────────────────┴────────┴─────────────────────────┘
#
# Total: 3ms | 3 passed, 0 failed
```

---

## Exit Codes

| Code | Meaning | CI Behavior |
|------|---------|-------------|
| 0 | All tests passed | Build succeeds |
| 1 | One or more tests failed | Build fails |
| 2 | Configuration error | Build fails |
| 3 | Trace file not found | Build fails |

---

## Environment Variables

| Variable | Description |
|----------|-------------|
| `ASSAY_CONFIG` | Default config file if `--config` not specified |
| `ASSAY_DB` | Default database path |
| `NO_COLOR` | Disable colored output |

---

## GitHub Actions Example

```yaml
name: Agent Quality Gate

on: [push, pull_request]

jobs:
  assay:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      
      - name: Install Assay
        run: cargo install assay
      
      - name: Run Tests
        run: |
          assay run \
            --config mcp-eval.yaml \
            --trace-file traces/golden.jsonl \
            --strict \
            --output sarif \
            --db :memory:
      
      - name: Upload SARIF
        uses: github/codeql-action/upload-sarif@v2
        if: always()
        with:
          sarif_file: .assay/reports/results.sarif
```

---

## See Also

- [assay import](import.md)
- [Configuration](../config/index.md)
- [CI Integration](../getting-started/ci-integration.md)
