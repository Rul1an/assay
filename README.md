# Assay

**Deterministic evaluation and regression gating for LLM applications.**

[![CI](https://github.com/Rul1an/assay/actions/workflows/assay.yml/badge.svg)](https://github.com/Rul1an/assay/actions)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## Overview

Assay is a local-first tool for evaluating AI agents and RAG pipelines. It provides strict regression gating in CI without requiring network access, ensuring deterministic repeatable builds.

*   **Performance**: Native Rust binary. No Python runtime overhead for the evaluation engine.
*   **CI Native**: Deterministic replay mode prevents flaky tests and enables offline regression gating.
*   **MCP Support**: Native integration with the **Model Context Protocol**. Import inspector/JSON-RPC transcripts directly.
*   **Observability**: OpenTelemetry ingestion and a real-time Terminal UI (TUI) for debugging.
*   **Architecture**: Typesafe SQLite storage for all traces and results.

---

## Quick Start

### 1. Install
Build the CLI from source:
```bash
cargo install --path crates/assay-cli
```

### 2. Run a Regression Gate
Compare a candidate trace against a baseline:
```bash
assay ci \
  --config examples/ci-regression-gate/eval.yaml \
  --trace-file examples/ci-regression-gate/traces/main.jsonl \
  --baseline examples/ci-regression-gate/baseline.json \
  --strict
```

### 3. MCP Gate (Model Context Protocol)
Validate an agent transcript from Anthropic Inspector or JSON-RPC logs:
```bash
# 1. Import trace
assay trace import-mcp \
    --input session.json \
    --format inspector \
    --test-id mcp_test_1 \
    --out-trace trace.v2.jsonl

# 2. Run Gate (Ephemeral DB)
assay ci \
    --config assay.yaml \
    --trace-file trace.v2.jsonl \
    --replay-strict \
    --db :memory:
```

### 4. Interactive TUI
Run the reference agent implementation:
```bash
cd examples/agent-demo-2
uv pip install -r requirements.txt
python demo_tui.py
```

```text
┌──────────────── Assay TUI ────────────────┐
│ 🟢 Status: Running   ⚡ Steps: 12/sec       │
│                                             │
│ > Thinking...                               │
│ > Calling tool: search_web("rust ci")       │
│ > Observation: Rust CI is fast...           │
│                                             │
│ [Dashboard] [Trace] [Metrics]               │
└─────────────────────────────────────────────┘
```
*(Run the demo to see the full high-fidelity interface)*


---

## Repository Structure

*   **`crates/`**: Core Rust implementation.
    *   `assay-core`: Evaluation engine, storage, and OTel ingestion.
    *   `assay-cli`: CLI entrypoint.
    *   `assay-metrics`: Shared metric definitions.
*   **`examples/`**: Reference implementations.
    *   `agent-demo-2/`: Interactive agent with TUI and attack simulation.
    *   `ci-regression-gate/`: Complete CI/CD workflow examples.
    *   `rag-grounding/`, `negation-safety/`: Metric configuration examples.

---

## CI Integration

Assay is designed to run in standard CI pipelines.

```yaml
# .github/workflows/ci.yml
jobs:
  assay-gate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Build Assay
        run: cargo build --release
      - name: Run Gate
        env:
          VERDICT_BIN: ./target/release/assay
        run: |
          $VERDICT_BIN ci --config eval.yaml --trace-file traces/pr.jsonl --baseline baseline.json --strict
```

---

## Design Principles

1.  **Strict Determinism**: Tests must be repeatable. Flakiness is a failure.
2.  **Local-First, CI-Compatible**: The same binary runs locally and in CI.
3.  **Type Safety**: Configuration and metrics are strictly validated.

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
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
