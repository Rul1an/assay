# ADR-002: Trace Replay as Input Adapter

## Status
Accepted

## Context
Live LLM calls in CI/CD are problematic due to cost, nondeterminism and latency.
We need to run the exact same evaluation logic against recorded interactions.

## Decision
We implement a **Trace Replay** mode where `assay` accepts a trace file (JSONL) as the backend instead of a live provider.

### 1. Contract & Schema
The trace file MUST be JSONL. Each line MUST be a valid JSON object conforming to **Trace Schema v1**:

```json
{
  "schema_version": 1,
  "type": "assay.trace",
  "request_id": "String (Optional) - Stable unique id",
  "prompt": "String (Required)",
  "context": ["String (Optional) - RAG context chunks"],
  "response": "String (Required)",
  "model": "String (Optional)",
  "provider": "String (Optional)",
  "meta": "Object (Optional)"
}
```

**Validation Rules**:
- **Schema Version**: If present, must be `1`.
- **Type**: If present, must be `assay.trace`.
- **Content**: One of `text` or `response` is REQUIRED. Empty strings break the contract if implied as successful response.

**Matching & Uniqueness**:
- **Lookup**: Traces are indexed by `prompt` to support the current `eval.yaml` contract.
- **Uniqueness**:
  - If `request_id` is present, it MUST be unique across the file.
  - The `prompt` MUST also be unique across the file to ensure deterministic lookup. (Ambiguous prompts = Error).

### 2. Privacy & Redaction
Traces can contain PII.
- **Default**: Prompts are kept for debugging.
- **Redaction**: When `--redact-prompts` is set, prompt text MUST be replaced with `[REDACTED]` in all outputs.

### 3. CI Workflow
Recommended workflow:
1.  **Dev/Staging**: record fresh traces.
2.  **Store**: commit sanitized traces.
3.  **PR Gate**: `assay ci --trace-file traces.jsonl`.
4.  **Drift Mitigation**: periodic re-record jobs.
