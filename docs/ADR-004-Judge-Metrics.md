# ADR-004 v2: Judge Metrics Strategy

## Status
Accepted (v2)

## Context
LLM-as-judge is essential for RAG evaluation (faithfulness/relevancy), but introduces variance, costs, and privacy risks. Since Assay is CI-first, judge-metrics must not undermine the reliability of the gate.

## Decision

### A. Architecture: Enrichment Pattern + Stateless Metrics
**Decision**: Retain the enrichment pattern.
The `Runner` handles replay, caching, voting, timeouts, and error mapping, injecting results into `resp.meta.assay.judge`. The metric implementations (`faithfulness`, etc.) read solely from this metadata.
**Rationale**: Prevents infrastructure duplication per metric, maximizes reuse, and keeps metrics purely functional/testable.

### B. CLI DX: Short Flags + Env Var Fallback
**Decision**: Adopt short flags and environment variable precedence.
**Precedence**: CLI flags > Env vars > Defaults.

**New Flags**:
*   `--judge <none|openai|fake>`: Default `none` (explicit). Alias `--no-judge`.
*   `--judge-model <string>`
*   `--judge-samples <u32>`: Default 3.
*   `--judge-refresh`: Force refresh/ignore cache.
*   (Future): `--judge-api-key`.

**Env Vars**:
*   `VERDICT_JUDGE`
*   `VERDICT_JUDGE_MODEL`
*   `VERDICT_JUDGE_SAMPLES`
*   `VERDICT_JUDGE_TEMPERATURE`
*   `VERDICT_JUDGE_MAX_TOKENS`

**Rationale**: Major DX improvement for daily use; enables "set once" workflows.

### C. Config Naming: `min_score`
**Decision**: Use `min_score` instead of `threshold`.
```yaml
expected:
  type: faithfulness
  min_score: 0.85
  rubric_version: v1
  samples: 3
```
**Rationale**: Consistent with `min_floor` (ADR-005) and semantically unambiguous.

### D. Determinism: Voting Defaults
**Decision**: Default `k=3` (balance cost/adoption).
*   Documentation will recommend `k=5` for critical production paths.
*   No early-exit optimization in MVP (keeps reasoning simpler).

### E. Cache Key Structure
**Decision**: Extend cache key to ensure reproducibility.
**Key Components**:
*   Provider, Model
*   Rubric ID, Rubric Version
*   Temperature, Max Tokens
*   Samples (`k`)
*   Input Hash (Prompt + Answer + Context)
*   Prompt Template Hash

**Rationale**: Prevents "accidental" cross-run cache hits that are not strictly reproducible.

### F. Error Messages & Timeouts
**Decision**: Actionable errors are must-have.
*   **Missing API Key**: Exit code 2.
*   **Cache Miss + No Judge**: Exit code 2 with instructions.
*   **Disagreement**: Status `Warn` (default), `Fail` under `--strict`.

**Timeout**: Reuse global `settings.timeout_seconds` for MVP. Future split to `judge_timeout_seconds`.

### G. Exit Codes
**Decision**: Do NOT implement exit code 3 ("unstable") in MVP.
**Codes**:
*   `0`: OK
*   `1`: Test Failures (including strict-mode Warn/Flaky)
*   `2`: Config/Setup/Runtime Error
**Rationale**: CI ecosystem expects `1 = fail`. "Unstable" states are handled via status + strict semantics without complicating platform integration.

### H. Trace Schema: Source, Samples, Agreement
**Decision**: Enrich trace metadata.
```json
"meta": {
  "assay": {
    "judge": {
      "faithfulness": {
        "rubric_version": "v1",
        "passed": true,
        "score": 0.92,
        "source": "trace",
        "samples": [true, true, false],
        "agreement": 0.67,
        "rationale": "..."
      }
    }
  }
}
```

## Consequences
*   Improved DX via concise flags and env fallbacks.
*   Reproducible caching guarantees.
*   Simplified CI semantics (no exit code 3).
