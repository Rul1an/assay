# ADR-005 v2: Relative Thresholds & Baselines

## Status
Accepted (v2)

## Context
Absolute thresholds (e.g., “0.85”) create adoption friction and are hard to justify across teams and domains. For most CI gates, the intended question is not “is this good enough in absolute terms?”, but “did this regress compared to **main** (or another baseline)?”.

Assay therefore needs a **baseline-driven** workflow where:
- teams can pin a known-good behavior (baseline),
- PR runs compare against that baseline using **relative thresholds**,
- missing baseline data is actionable and non-blocking by default (unless strict).

This ADR focuses on baseline DX, config ergonomics, and compatibility guarantees.

## Decision

### A. DX: Combined Baseline Workflow (Primary)
**Decision:** Reduce command surface area to a “1-2 punch”.

- **Generate baseline (main branch):**
  ```bash
  assay ci --export-baseline baseline.json --strict
  ```

- **Gate against baseline (PR):**
  ```bash
  assay ci --baseline baseline.json --strict
  ```

- **Advanced tool (non-primary):**
  `assay compare` remains available for offline comparison, debugging, and custom pipelines, but is not the onboarding path.

**Rationale:** Friction kills adoption. Baselines must feel like “normal CI”.

**Flag interaction rule:**
- Passing both `--baseline` and `--export-baseline` in the same command is a **Config Error (Exit 2)**.
- **Rationale:** “compare + overwrite baseline” is easy to misuse and creates unsafe workflows.

### B. Config: Suite Defaults + Per-Test Override

**Decision:** Allow centralized defaults under `settings.thresholding`, with per-test overrides under `expected.thresholding`.

```yaml
settings:
  thresholding:
    mode: relative
    max_drop: 0.03
    min_floor: 0.80

tests:
  - id: "critical_rag"
    expected:
      type: semantic_similarity_to
      text: "..."
      # uses suite defaults

  - id: "experimental_feature"
    expected:
      type: semantic_similarity_to
      text: "..."
      thresholding:
        max_drop: 0.10
```

**Rationale:** 80%+ of tests should not need repeated threshold boilerplate.

**Applicability rule (important):**
Relative thresholding applies only to metrics with a numeric score (e.g., `semantic_similarity_to`, judge metrics that produce a score).
Pure pass/fail metrics (e.g., `regex_match`, `json_schema`, `must_contain`) are not baseline-gated unless explicitly designed to emit scores in a future ADR.

### C. Missing Baseline Behavior

**Decision:** Missing baseline data is non-blocking by default, but actionable.

- **Default behavior:** Mark as `Warn` (Exit 0) and emit an actionable message:
  ```text
  Warning: No baseline entry for test '<test_id>' metric '<metric_name>'.
    This test will run, but no regression check is applied.
    To create a baseline: assay ci --export-baseline baseline.json --strict
    To enforce baselines: run with --strict (or future --require-baseline)
  ```

- **Strict mode (`--strict`):** Missing baseline becomes `Fail` (Exit 1).
- **Rationale:** Strict mode is “zero tolerance” and can be used to enforce baseline completeness without introducing a new exit code class.
- **Future (deferred):** `--require-baseline` to enforce baseline presence independently of `--strict`.

### D. Baseline JSON Schema & Compatibility Guarantees

**Decision:** Baseline files are versioned and self-describing, with strict compatibility checks.

**Baseline schema (v1):**
```json
{
  "schema_version": 1,
  "suite": "demo_suite",
  "assay_version": "0.1.0",
  "created_at": "2025-12-21T12:00:00Z",
  "config_fingerprint": "sha256:<hash>",
  "entries": [
    {
      "test_id": "rag_q1",
      "metric": "semantic_similarity_to",
      "score": 0.91,
      "meta": {
        "model": "text-embedding-3-small",
        "rubric_version": "v1"
      }
    }
  ]
}
```

**Required fields:**
- `schema_version` (u32)
- `suite` (string)
- `assay_version` (string)
- `created_at` (RFC3339 string, UTC recommended)
- `config_fingerprint` (string, sha256: prefix recommended)
- `entries[]` (array)

Each `entries[]` item MUST include:
- `test_id` (string)
- `metric` (string)
- `score` (number)

Optional per entry:
- `meta` (object) - for audit/debug (embedding model, dims, rubric_version, etc.)

**Compatibility policy:**
- `schema_version` mismatch -> **Config Error (Exit 2)**
  - Must include a message instructing user to regenerate baseline or upgrade tooling.
- `suite` mismatch -> **Config Error (Exit 2)**
  - Prevents accidentally comparing unrelated suites.
- `config_fingerprint` mismatch -> **Warn by default; Fail under `--strict`**
  - Rationale: suite may match, but config/test definitions may have changed.
- `assay_version` mismatch -> **Warn** (do not block by default)
  - Rationale: minor version drift may still be comparable; warn for awareness.

**config_fingerprint definition (v1):**
`config_fingerprint` is the SHA256 of a canonicalized representation of:
- the eval config file contents (after path normalization),
- and a metric “version set” (e.g., metric names + internal versions where applicable).

Exact canonicalization is an implementation detail, but must be:
- stable across platforms,
- deterministic across runs.

**Rationale:** Baseline comparisons should be meaningful; fingerprint helps detect “baseline from a different config”.

### Gate Semantics with Baselines

When `--baseline` is provided:
1. Assay runs the suite normally (respecting replay/live, redaction, strict mode, etc.).
2. For each scored metric result:
   - If baseline entry exists -> compute delta and evaluate thresholding rules.
   - If baseline missing -> apply Missing Baseline behavior (Warn by default).
3. Assay final status/exit code follows ADR-003 semantics:
   - Fail/Error -> Exit 1
   - Warn/Flaky -> Exit 0 (unless `--strict`, then Exit 1)
   - Config errors (schema mismatch, invalid baseline) -> Exit 2

No additional exit code class is introduced.

## Consequences
- Baselines become the “normal” workflow for regression gating.
- Threshold configuration remains compact via suite-level defaults.
- Users get actionable guidance when baselines are missing.
- Compatibility problems are detected early (schema/suite mismatch hard fails).

## Examples

### Scenario 1: Regression (Fail)
**Baseline (baseline.json):**
Test `q_1`, semantic_similarity, score: 0.92

**Current Run:**
Test `q_1`, score: 0.85
Config: `max_drop: 0.05`

**Logic:**
Delta = 0.85 - 0.92 = -0.07.
Drop (-0.07) exceeds max allowed (-0.05).

**Output:**
```text
FAIL [q_1]: regression detected: semantic_similarity_to dropped 0.07 (max allowed: 0.05)
```

### Scenario 2: Improvement (Pass)
**Baseline:** Score 0.80
**Current:** Score 0.82
**Logic:** Delta +0.02. Pass.

## Roadmap
- **PR11**: Baseline logic, Schema, CLI arguments.
- **PR12**: (Deferred) Statistical gating, auto-updates.
