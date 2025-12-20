# Verdict

**Verdict** is a CI-first **PR regression gate** for RAG pipelines. It helps you verify LLM-based applications deterministically in CI/CD without accruing massive bills or dealing with flake.

## Features (MVP v0.2.0)

- **Trace Injection** (`--trace-file`): Replay production/staging logs completely offline. No API keys required in CI.
- **OpenTelemetry Export** (`--otel-jsonl`): Emits evaluation results using [GenAI Semantic Conventions](https://opentelemetry.io/docs/specs/semconv/gen-ai/).
- **VCR Caching**: Caches live LLM responses to SQLite for fast local iteration.
- **JUnit/SARIF Support**: Native integration with GitHub Actions, GitLab CI, etc.

## Installation

```bash
cargo install --path crates/verdict-cli
```

## Quick Start

### 1. Initialize
```bash
verdict init
```
This fails if `eval.yaml` already exists.

### 2. Run (Live Mode)
```bash
# Requires dummy client (default) or configured provider
verdict run --config eval.yaml
```

### 3. Run (Trace Replay Mode) - Recommended for CI
```bash
# Uses a JSONL file as the source of truth.
# Errors if prompts are missing or duplicated.
verdict ci --trace-file examples/traces.jsonl
```

### 4. CI Output (OTel + SARIF)
```bash
verdict ci \
  --trace-file traces.jsonl \
  --junit report.xml \
  --sarif results.sarif \
  --otel-jsonl telemetry.jsonl
```

## Trace Schema
See [ADR-002](docs/ADR-002-Trace-Replay.md) for details.
```json
{"prompt": "...", "response": "..."}
```

## License
MIT / Apache-2.0
