# Troubleshooting

This document is the “fast path” for fixing Assay issues in CI.
If you’re stuck: run `assay validate` first, then `assay doctor` (v0.3.4+).

## Quick Triage (90 seconds)
1) **Preflight**
```bash
assay validate --config eval.yaml --trace-file traces/ci.jsonl
```

2) **If you use offline CI**
```bash
assay validate --config eval.yaml --trace-file traces/ci.jsonl --replay-strict
```

3) **If you gate against baseline**
```bash
assay validate --config eval.yaml --trace-file traces/ci.jsonl --baseline baseline.json
```

Exit codes refresher:
*   `0`: OK (Pass/Warn/Flaky/Skipped)
*   `1`: Test failures (regressions)
*   `2`: Config/setup errors (paths, schema mismatch, replay-strict missing data, etc.)

---

## Failure Mode 1 — Trace miss (prompt drift)

**Symptom**
*   Runner/validate can’t find a prompt in the trace.
*   Diagnostic: `E_TRACE_MISS` with “Did you mean…?” suggestion.

**Likely cause**
*   Prompt template changed (typo, spacing, new prefix), but trace dataset wasn’t updated.

**Fast fix**
*   Update `input.prompt` in config to exactly match trace or regenerate trace.
*   If you intended the new prompt: re-ingest + re-precompute (if strict/offline):
    ```bash
    assay trace ingest --input raw_logs/*.jsonl --output trace.jsonl
    assay trace precompute-embeddings --trace trace.jsonl --output trace_enriched.jsonl --embedder openai
    assay trace precompute-judge --trace trace_enriched.jsonl --output trace_enriched.jsonl --judge openai
    ```

**Prevention**
*   Treat prompts as API contracts: version prompt templates, update traces on main.

- **Relevant Diagnostic**: `W_CACHE_CONFUSION`
- **Verify**: `assay doctor` (Check "Caches" section)
- **Fast Fix**:
    1.  **Use `assay-action@v0.3.4+`** which splits caches automatically.
- **Relevant Diagnostic**: `E_BASE_MISMATCH`
- **Verify**: `assay validate --config eval.yaml --baseline baseline.json`
- **Fast Fix**:
    1.  If the change is intentional (e.g., prompt update), **export a new baseline**:
        ```bash
        assay ci --config eval.yaml --trace-file traces/pr.jsonl --export-baseline baseline.json --strict
        ```
- **Relevant Diagnostic**: `E_TRACE_MISS`, `E_TRACE_INVALID`
- **Verify**: `assay validate --config eval.yaml --trace-file traces/pr.jsonl`
- **Fast Fix**:
    1.  Ensure your app logs inputs/outputs in JSONL format.
*   Codes: `E_TRACE_MISS`, `E_TRACE_INVALID`
*   Commands: `assay validate`, `assay trace verify`

---

## Failure Mode 2 — Baseline churn (too many diffs / constant red PRs)

**Symptom**
*   PRs fail frequently due to “regression vs baseline”, but changes are expected.
*   Team starts ignoring the gate.

**Likely cause**
*   Baseline exported from a run that isn’t representative (too small, wrong trace slice, unstable judge).
*   `max_drop` too strict for the metric noise profile.

**Fast fix**
*   Run calibration on historical data:
    ```bash
    assay calibrate --db .eval/eval.db --suite <suite> --out calibration.json
    ```
*   Use recommended thresholds (`recommended_min_score`, `recommended_max_drop`) for your suite.
*   Use hygiene report to identify unstable tests:
    ```bash
    assay baseline report --db .eval/eval.db --suite <suite> --out hygiene.json
    ```

**Prevention**
*   Export baseline on main only, from a stable “golden” dataset.
*   Quarantine or relax thresholds for unstable tests.

**Relevant**
*   Commands: `assay ci --export-baseline ...`, `assay calibrate`, `assay baseline report`

---

## Failure Mode 3 — Schema/version drift (baseline / trace / db)

**Symptom**
*   Hard config errors after upgrades, or baseline refuses to load.
*   Typical: suite mismatch, schema version mismatch.

**Likely cause**
*   Baseline from a different suite or incompatible schema.
*   Upgraded binary, but reused an old baseline/db without migration.

**Fast fix**
*   Regenerate baseline for the correct suite:
    ```bash
    assay ci --config eval.yaml --trace-file traces/main.jsonl --export-baseline baseline.json --strict
    ```
*   Run validate with baseline to get actionable diagnostics:
    ```bash
    assay validate --config eval.yaml --baseline baseline.json
    ```

**Prevention**
*   Store baseline next to config and update it like any other golden artifact.

**Relevant**
*   Codes: `E_BASE_MISMATCH`, `E_PATH_NOT_FOUND`

---

## Failure Mode 4 — Cache confusion (masked failures / “it passes on CI but not locally”)

**Symptom**
*   Behavior differs between runs with identical inputs.
*   Tests unexpectedly “SKIPPED fingerprint match” after meaningful changes.

**Likely cause**
*   Shared `.eval` DB across unrelated runs or incorrect cache keys in CI.

**Fast fix**
*   Nuke local cache:
    ```bash
    rm -rf .eval ~/.assay/cache ~/.assay/embeddings
    ```
*   In CI, ensure cache split is enabled (v0.3.4 action defaults should handle this).

**Prevention**
*   Use per-workdir cache keys (monorepo safe).
*   Keep `.eval` cache separate from runtime caches.

**Relevant**
*   Commands: `assay ci --incremental`, `--refresh-cache`
*   Action: cache split (db vs runtime)

---

## Failure Mode 5 — Embedding dimension mismatch

**Symptom**
*   Config error: embedding dims mismatch (e.g. 1536 vs 3072) or empty vectors.
*   Validate shows: `E_EMB_DIMS`.

**Likely cause**
*   Mixed embedding models between trace precompute runs.
*   Old precomputed embeddings reused after changing embedder model.

**Fast fix**
*   Recompute embeddings with the intended model:
    ```bash
    assay trace precompute-embeddings --trace trace.jsonl --output trace_enriched.jsonl --embedder openai --model <your-model>
    ```

**Prevention**
*   Pin embedder model in docs / pipeline.
*   Treat embedding model as part of the dataset fingerprint.

**Relevant**
*   Codes: `E_EMB_DIMS`
*   Commands: `assay validate --replay-strict`

---

## Failure Mode 6 — Judge variance confusion (Warn/Flaky/Unstable)

**Symptom**
*   “Warn / Unstable” shows up; team doesn’t know if it’s safe to merge.
*   Disagreement across samples.

**Likely cause**
*   Judge sampling (k>1) reveals borderline cases.
*   Temperature/model changes in judge config.

**Fast fix**
*   In early adoption: allow Warn (exit 0) but track it.
*   For strict gating: use `--strict` in CI.
*   Use hygiene report to locate the unstable tests and either:
    *   lower strictness for that metric/test, or
    *   improve rubric / grounding.

**Prevention**
*   Keep judge prompts/rubrics versioned (`rubric_version`).
*   Keep judge temperature stable.

**Relevant**
*   `--strict`, hygiene report, calibration

---

## Failure Mode 7 — Fork PR permissions (SARIF/artifact uploads fail)

**Symptom**
*   Action fails on fork PRs due to permissions; SARIF upload errors.

**Likely cause**
*   GitHub restricts token permissions on forks.

**Fast fix**
*   Use `sarif: auto` (default): auto-skips SARIF on fork PRs.
*   Or explicitly disable:
    ```yaml
    with:
      sarif: false
    ```

**Prevention**
*   Keep SARIF “best effort” for forks; require it only on main branch workflows.

---

## Failure Mode 8 — Monorepo path resolution issues

**Symptom**
*   “file not found” for config/trace/baseline, but paths look correct.

**Likely cause**
*   Wrong working directory in Action vs repo layout.

**Fast fix**
```yaml
with:
  workdir: packages/ai
  config: eval.yaml
  trace_file: traces/ci.jsonl
```

**Prevention**
*   Always set `workdir` in monorepos; keep config-relative assets.

---

## Failure Mode 9 — Large trace performance (slow CI)

**Symptom**
*   CI takes too long; heavy JSONL parsing or repeated embedding/judge work.

**Likely cause**
*   Too large dataset, missing incremental skip, or missing precompute caches.

**Fast fix**
*   Enable incremental:
    ```bash
    assay ci --incremental ...
    ```
*   Use precompute + `replay-strict` (offline deterministic).
*   Ensure action runtime caches are enabled (`cache_mode: auto`).

**Prevention**
*   Keep a “CI slice” dataset + a larger nightly dataset.

---

## Failure Mode 10 — “No idea what to do next”

**Symptom**
*   Users get an error but don’t know the next command to run.

**Fix**
*   Always start with:
    ```bash
    assay validate --config eval.yaml --trace-file traces/ci.jsonl --baseline baseline.json --replay-strict
    ```
*   Then follow the Diagnostic’s `fix_steps`.

**Prevention**
*   Keep docs and examples in-repo; link this page from README.
