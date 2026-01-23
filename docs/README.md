# Assay

**Zero-flake regression testing for AI agents.**

Assay replaces flaky, network-dependent evals with deterministic replay testing. Record your agent's behavior once, then validate every PR in milliseconds — no API calls, no cost, no surprises.

## Why Assay?

| Problem | Traditional Evals | Assay |
|---------|-------------------|-------|
| CI flakiness | LLM calls fail randomly | Deterministic replay |
| Cost | $$ per test run | $0 after recording |
| Speed | Seconds per test | Milliseconds |
| Privacy | Data sent to cloud | 100% local |

## 5-Minute Quickstart

### 1. Install

```bash
cargo install assay-cli
```

### 2. Create a config

```yaml
# mcp-eval.yaml
configVersion: 1
suite: my_agent

tests:
  - id: basic_flow
    input:
      prompt: "Deploy to staging"
    expected:
      type: args_valid
      schema:
        deploy_service:
          type: object
          required: [env]
          properties:
            env:
              type: string
              enum: [staging, prod]
```

### 3. Import a trace

```bash
# From MCP Inspector logs
assay import --format mcp-inspector session.json --out-trace trace.jsonl
```

### 4. Run your first eval

```bash
assay run --config mcp-eval.yaml --trace-file trace.jsonl

# Output:
# Running 1 tests...
# ✅ basic_flow        passed (0.2s)
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# Summary: 1 passed, 0 failed, 0 skipped
```

### 5. Add to CI

```yaml
# .github/workflows/eval.yaml
name: Agent Evaluation

on: [pull_request]

jobs:
  eval:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Assay
        run: cargo install assay-cli

      - name: Run evals
        run: assay run --config mcp-eval.yaml --trace-file traces/golden.jsonl --strict
```

## Core Concepts

### Traces

A trace is a recording of your agent's behavior — every tool call, every argument, every response. Assay replays these traces deterministically.

```jsonl
{"type":"tool_call","tool":"deploy_service","args":{"env":"staging"}}
{"type":"tool_result","result":{"status":"success"}}
```

### Policies

Policies define what "correct" behavior looks like:

- **`args_valid`** — Tool arguments match a JSON Schema
- **`sequence_valid`** — Tools are called in the right order
- **`tool_blocklist`** — Certain tools are never called
- **`regex_match`** — Output matches a pattern

### Golden Traces

A "golden trace" is a known-good recording that becomes your regression baseline. When behavior changes, Assay catches it.

## Documentation

- [CLI Reference](./CLI_REFERENCE.md) — All commands and flags
- [Config Reference](./CONFIG_REFERENCE.md) — Full `mcp-eval.yaml` schema
- [Troubleshooting](./TROUBLESHOOTING.md) — Common errors and fixes
- [Migration Guide](./MIGRATION.md) — Upgrading from v0 configs

## Use Cases

### CI Regression Gate

Prevent prompt changes from breaking existing capabilities:

```bash
assay run --config mcp-eval.yaml --trace-file goldens.jsonl --strict
```

### Trace-Driven Debugging

Reproduce and fix user-reported failures:

```bash
assay import --format mcp-inspector user_bug.json --out-trace bug.jsonl
assay run --config mcp-eval.yaml --trace-file bug.jsonl
```

### Agent Self-Correction (Runtime)

Let your agent validate its own actions before executing:

```python
# Agent calls Assay before executing
result = assay_check_args(tool="deploy_service", args={"env": "prod"})
if not result.valid:
    # Self-correct based on error message
    ...
```

## License

MIT
