# OpenTelemetry & Langfuse

Assay does not try to replace your observability stack.

Use Langfuse, OTel collectors, or your existing tracing pipeline for live visibility.
Use Assay when you want to turn those traces into:

- deterministic replay input
- policy gates in CI
- tamper-evident evidence bundles for audit handoff

## The Flow

```text
Agent framework -> OTel / Langfuse -> JSONL export -> assay trace ingest-otel -> Assay replay + evidence
```

## 1. Export OpenTelemetry JSONL

Assay's OTel ingest path expects OpenTelemetry-style JSONL spans aligned with GenAI semantic conventions.

At minimum, emit:

- `gen_ai.prompt`
- `gen_ai.tool.name`
- `gen_ai.tool.args`
- `gen_ai.completion`

If your stack already sends spans to Langfuse, keep doing that. Assay can consume the same exported trace data as a downstream governance step.

## 2. Ingest Into Assay

```bash
assay trace ingest-otel \
  --input otel-export.jsonl \
  --db .eval/eval.db \
  --suite checkout-agent \
  --out-trace traces/otel.v2.jsonl
```

What this gives you:

- a normalized Assay trace dataset in SQLite for structural assertions
- an optional replay trace file for deterministic CI runs

## 3. Gate and Replay

```bash
assay ci \
  --config eval.yaml \
  --db .eval/eval.db \
  --trace-file traces/otel.v2.jsonl \
  --replay-strict
```

`--replay-strict` keeps the run offline and deterministic. If a prompt is missing from the trace file, the run fails instead of calling a live model.

## 4. Export Evidence

```bash
assay evidence export --profile profile.yaml --out evidence.tar.gz
assay evidence verify evidence.tar.gz
```

Now you have both:

- observability in your existing stack
- a replayable, verifiable evidence artifact in Assay

## Langfuse Positioning

Langfuse is great for tracing, prompts, and production observability.
Assay sits next to it:

- Langfuse answers: "What happened in production?"
- Assay answers: "Was this tool call allowed, reproducible, and audit-ready?"

## See Also

- [Testing Agents with Assay](../architecture/agents.md)
- [MCP Quick Start](../mcp/quickstart.md)
- [Policy Files](../reference/config/policies.md)
