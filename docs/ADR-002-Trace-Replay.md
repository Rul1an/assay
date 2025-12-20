# ADR-002: Trace Replay as Input Adapter

## Status
Accepted

## Context
For CI/CD pipelines, calling live LLM providers (OpenAI, Anthropic, etc.) is problematic due to:
1.  **Cost**: Running extensive regression suites on every commit is expensive.
2.  **Determinism**: LLMs are non-deterministic; flaky tests erode trust in the gate.
3.  **Latency**: Live calls are slow.

We need a way to run the *exact same* evaluation logic (metrics, assertions, reporting) against a recorded set of interactions.

## Decision
We implement a **Trace Replay** mode where `verdict` accepts a static trace file (JSONL) as the "backend" instead of a live LLM client.

### 1. Contract & Schema
The input trace file MUST be a JSONL file. Each line MUST be a valid JSON object conforming to the following schema (simplified):

```json
{
  "prompt": "String (Required) - Used as the unique lookup key",
  "response": "String (Required) - The text to be evaluated",
  "model": "String (Optional) - Model identifier for metadata",
  "provider": "String (Optional) - Provider identifier",
  "meta": "Object (Optional) - Arbitrary metadata passed through to results/OTel"
}
```

**Matching Rule**:
- **Exact Match**: The prompt in the `eval.yaml` test case MUST match the `prompt` field in the trace file character-for-character.
- **Uniqueness**: The trace file MUST NOT contain duplicate prompts. If duplicates are detected during load, the process MUST exit with an error. This ensures strict determinism (no "first-match-wins" ambiguity).

### 2. Privacy & Redaction
Since traces and their evaluation outputs are exported (e.g., via OTel), PII leakage is a risk.
- **Default**: Prompts are stored/exported to aid debugging.
- **Redaction Mode**: When the `--redact-prompts` flag is set, the OTel export MUST replace prompt text with `[REDACTED]`.

### 3. CI Workflow
The recommended workflow is:
1.  **Dev/Staging**: Run live `verdict run` to generate fresh traces (future feature: `verdict record`).
2.  **Commit**: Check in sanitized traces (or upload to artifact store).
3.  **CI (PR Gate)**: Run `verdict ci --trace-file traces.jsonl` to verify regressions against the pinned behavior.

## Consequences
- **Positive**: Zero cost CI runs, 100% stable baselines, offline capable.
- **Negative**: "Stale" traces might verify behavior that is no longer true for the live model (model drift). This requires periodic "re-record" jobs.
