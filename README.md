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

## Contributing

```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
