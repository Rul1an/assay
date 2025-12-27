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

## 🔌 Model Context Protocol (MCP) Integration

Assay supports testing MCP servers by importing Inspector transcripts or JSON-RPC logs.

1.  **Import & Init**: Convert a transcript into a trace and generate evaluation scaffolding.
    ```bash
    assay import --format mcp-inspector my_session.json --init
    ```
    This creates `mcp-eval.yaml` with **inline policies** for arguments and tool sequences.

2.  **Verify**: Replay the trace strictly to ensure the server behaves deterministically.
    ```bash
    assay run --config mcp-eval.yaml --trace-file my_session.trace.jsonl --replay-strict
    ```

3.  **Harden**: Tweak the inline JSON Schemas in `mcp-eval.yaml` to enforce strict contracts.

> **Legacy Migration**: If you have an older project with separate policy files (`policies/`), run:
> ```bash
> assay migrate --config old_config.yaml
> ```
> This will inline all external policies and update the configuration to `configVersion: 1`.
>
> **Legacy Mode**: By default, Assay v0.8+ enforces strict configuration versioning. To temporarily run legacy v0 configurations without migrating, set `MCP_CONFIG_LEGACY=1`.

## Migration Guide (v0.8.0+)

Assay v0.8 introduces `configVersion: 1` to support strict inline policies and reproducible builds.

### 1. Auto-Migration
The easiest way to upgrade is using the CLI:

```bash
# Preview changes (dry run)
assay migrate --config my_eval.yaml --dry-run

# Apply changes (creates my_eval.yaml.bak)
assay migrate --config my_eval.yaml
```

This command will:
*   Read external policy files (e.g., `policies/args.yaml`)
*   Inline them directly into `mcp-eval.yaml`
*   Convert legacy list-based sequences to the new Rule DSL (`require`, `before`, `blocklist`)
*   Set `configVersion: 1`

### 2. Manual Changes & Edge Cases

If you prefer manual migration or encounter issues:

*   **Mixed Versions**: Assay supports executing v0 (legacy) and v1 (modern) tests in the same suite during the transition.
*   **YAML Anchors**: Standard YAML anchors are fully supported in v1 configs for sharing settings.
*   **Duplicate Tools**: The new Sequence DSL handles duplicate tool calls robustly. Use `rules` instead of raw lists.

### FAQ / Troubleshooting

**Q: My tests fail with "unsupported config version 0".**
A: Run `assay migrate` to upgrade, or set `MCP_CONFIG_LEGACY=1` environment variable to force legacy mode temporarily.

**Q: I have a huge `policies/` directory. Do I strictly need to inline everything?**
A: Inlining is recommended for reproducibility (Artifacts contain everything). However, v1 still supports `policy: path/to/file.yaml` for `args_valid` metrics if you really need it, but future tooling (GUI) may assume inlined schemas.

## Contributing

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
