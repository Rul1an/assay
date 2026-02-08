# Plan: Pipeline Decomposition (Post mod.rs Refactor)

> **Status**: Proposed
> **Date**: 2026-02-08
> **Predecessor**: mod.rs refactor (done — mod.rs now 46 lines, dispatch in own file)
> **RFC**: RFC-001-dx-ux-governance.md, Wave B1/B2
> **Constraint**: No behavior changes, no output-contract changes, `cargo test --workspace` green after each step.

---

## Problem

The mod.rs extraction succeeded (1173 → 46 lines), but created a new concentration point. `pipeline.rs` (557 lines) now combines 5 distinct concerns that were previously mixed in mod.rs. Additionally, `run.rs` and `ci.rs` contain identical error-handling and report-timing patterns.

## Verified Findings

### F1 — `pipeline.rs` is a new god-module (557 lines, 5 concerns)

| Lines | Concern | Target |
|-------|---------|--------|
| 11–91 | `PipelineInput` + `from_run`/`from_ci` | stays in pipeline.rs |
| 93–96 | `PipelineError` enum | → `pipeline_error.rs` |
| 123–310 | `execute_pipeline()` | stays in pipeline.rs |
| 312–334 | `write_error_artifacts()` | → `reporting.rs` |
| 336–364 | `build_summary_from_artifacts()` | → `reporting.rs` |
| 366–429 | `build_performance_metrics()` | → `reporting.rs` |
| 431–441 | `print_pipeline_summary()` | → `reporting.rs` |
| 443–454 | `maybe_export_baseline()` | → `reporting.rs` |
| 456–557 | Tests (performance metrics) | → `reporting.rs` |

**After extraction:** `pipeline.rs` drops from 557 → ~250 lines (input mapping + execution only).

### F2 — Identical error-handling block in run.rs and ci.rs

Lines 17–31 in both files are identical 15-line `match` blocks:

```rust
// run.rs:17 and ci.rs:17 — identical
let execution = match execution {
    Ok(ok) => ok,
    Err(PipelineError::Classified { run_error }) => {
        let reason = reason_code_from_run_error(&run_error)
            .unwrap_or(ReasonCode::ECfgParse);
        return write_error_artifacts(reason, run_error.message, ...);
    }
    Err(PipelineError::Fatal(err)) => return Err(err),
};
```

**Fix:** Method on `PipelineError` that encapsulates this mapping.

### F3 — Duplicate `elapsed_ms()` definitions

Two identical helper functions:
- `pipeline.rs:114–121` (7 lines, with bounds check)
- `profile.rs:488–490` (1-liner, inline `min()`)

Plus two inline occurrences of the same pattern:
- `run.rs:53`: `report_start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64`
- `ci.rs:104`: identical

**Fix:** One shared `elapsed_ms()` in a tiny util, used everywhere.

### F4 — Repetitive `PipelineError::Classified` construction (10 instances)

All 10 follow the same pattern in `execute_pipeline()`:
```rust
return Err(PipelineError::Classified {
    run_error: RunError::config_parse(Some(path), msg),
});
```

**Fix:** Small constructor methods on `PipelineError` (`cfg_err`, `missing_cfg`, `invalid_args`, `from_run_error`).

### F5 — Report-timing + summary rebuild duplication

Both `run.rs` and `ci.rs`:
1. Create `report_start = Instant::now()`
2. Write output formats (run.json, summary, etc.)
3. Measure `report_ms = elapsed(report_start)`
4. Rebuild summary with `build_summary_from_artifacts(..., Some(report_ms))`
5. Write final summary

The difference: ci.rs additionally writes JUnit, SARIF, OTel, PR comment. Both need the same summary finalization pattern.

**Fix:** `finalize_and_write_summary()` helper in `reporting.rs` that takes the pre-built summary and writes with timing.

---

## Target Structure

```
commands/
  mod.rs               (46 lines)  — unchanged
  dispatch.rs          (54 lines)  — unchanged
  pipeline.rs         (~250 lines) — PipelineInput + execute_pipeline only
  pipeline_error.rs    (~50 lines) — PipelineError + constructor methods + error→reason mapping
  reporting.rs        (~200 lines) — summary building, error artifacts, console output, perf metrics
  reporting_tests.rs  (~100 lines) — performance metric tests (from pipeline.rs)
  run.rs              (~40 lines)  — thin: input → pipeline → report
  ci.rs              (~140 lines)  — thin: input → pipeline → CI outputs → report
  run_output.rs       (445 lines)  — unchanged
  runner_builder.rs   (262 lines)  — unchanged
```

Alternatively, `reporting_tests.rs` can stay as `#[cfg(test)] mod tests` inside `reporting.rs`.

---

## Steps

### Step 1: Extract `pipeline_error.rs`

Move from `pipeline.rs`:
- `PipelineError` enum (lines 93–96)
- `elapsed_ms()` helper (lines 114–121) — make `pub(crate)`

Add constructor methods:
```rust
impl PipelineError {
    pub(crate) fn cfg_parse(path: impl Into<String>, msg: impl Into<String>) -> Self {
        Self::Classified {
            run_error: RunError::config_parse(Some(path.into()), msg.into()),
        }
    }

    pub(crate) fn missing_cfg(path: impl Into<String>, msg: impl Into<String>) -> Self {
        Self::Classified {
            run_error: RunError::missing_config(path.into(), msg.into()),
        }
    }

    pub(crate) fn invalid_args(msg: impl Into<String>) -> Self {
        Self::Classified {
            run_error: RunError::invalid_args(msg.into()),
        }
    }

    pub(crate) fn from_run_error(run_error: RunError) -> Self {
        Self::Classified { run_error }
    }

    /// Map pipeline error to exit code + write error artifacts.
    /// Shared between run.rs and ci.rs.
    pub(crate) fn into_exit_code(
        self,
        version: ExitCodeVersion,
        verify_enabled: bool,
        run_json_path: &Path,
    ) -> anyhow::Result<i32> {
        match self {
            Self::Classified { run_error } => {
                let reason = reason_code_from_run_error(&run_error)
                    .unwrap_or(ReasonCode::ECfgParse);
                write_error_artifacts(reason, run_error.message, version, verify_enabled, run_json_path)
            }
            Self::Fatal(err) => Err(err),
        }
    }
}
```

**Update `pipeline.rs`:** Replace 10 verbose `PipelineError::Classified { run_error: RunError::... }` constructions with one-liner constructors.

**Update `run.rs` and `ci.rs`:** Replace 15-line match block with:
```rust
let execution = match execute_pipeline(&input, legacy_mode).await {
    Ok(ok) => ok,
    Err(e) => return e.into_exit_code(version, !args.no_verify, &run_json_path),
};
```

**Verification:**
- `cargo build -p assay-cli`
- `cargo test -p assay-cli`
- Identical behavior: error exit codes and run.json/summary.json content unchanged

### Step 2: Extract `reporting.rs`

Move from `pipeline.rs`:
- `write_error_artifacts()` (lines 312–334)
- `build_summary_from_artifacts()` (lines 336–364)
- `build_performance_metrics()` (lines 366–429)
- `print_pipeline_summary()` (lines 431–441)
- `maybe_export_baseline()` (lines 443–454)
- `#[cfg(test)] mod tests` (lines 456–557)

All functions remain `pub(crate)`. Imports from `run_output` and `assay_core::report`.

**Update `pipeline.rs`:** Remove moved functions, add `use super::reporting::*` where needed (only `write_error_artifacts` if referenced by `pipeline_error.rs`).

**Update `run.rs` and `ci.rs`:** Change imports from `super::pipeline::` to `super::reporting::` for summary/printing/baseline functions.

**Verification:**
- `cargo build -p assay-cli`
- `cargo test -p assay-cli` (especially performance metric tests)

### Step 3: Deduplicate `elapsed_ms()`

- Remove `elapsed_ms()` from `pipeline.rs` (moved to `pipeline_error.rs` in step 1)
- Remove `elapsed_ms()` from `profile.rs:488–490`
- Both import from `pipeline_error::elapsed_ms`
- Replace inline occurrences in `run.rs:53` and `ci.rs:104` with `elapsed_ms(report_start)`

**Verification:**
- `cargo build -p assay-cli`
- `cargo test -p assay-cli -p assay-evidence`

### Step 4: Summary finalization helper (optional, only if Step 1–3 feel clean)

If after steps 1–3 the report-timing pattern in `run.rs` and `ci.rs` still feels duplicated, extract:

```rust
// reporting.rs
pub(crate) fn finalize_summary(
    base_summary: Summary,
    report_start: Instant,
    summary_path: &Path,
) -> anyhow::Result<()> {
    let report_ms = elapsed_ms(report_start);
    let summary = base_summary.with_report_ms(report_ms);
    write_summary(&summary, summary_path)
}
```

**Decision gate:** Only do this if run.rs and ci.rs still have >5 lines of identical summary-writing code after steps 1–3. If the duplication is small (3–4 lines), leave it — premature abstraction.

---

## What This Does NOT Change

- `run_output.rs` (445 lines) — separate concern (outcome decision + run.json formatting), no overlap with pipeline
- `runner_builder.rs` (262 lines) — separate concern (Runner construction), no overlap
- Any output contract (`run.json`, `summary.json`, SARIF, JUnit)
- Any exit code behavior
- Any test behavior

---

## Verification Checklist

- [ ] `cargo build -p assay-cli` compiles after each step
- [ ] `cargo test -p assay-cli` passes after each step (especially `performance_metrics_*` tests)
- [ ] `cargo clippy -p assay-cli -- -D warnings` clean after each step
- [ ] `pipeline.rs` < 260 lines after step 2
- [ ] `run.rs` < 50 lines, `ci.rs` < 150 lines after step 1
- [ ] No `elapsed_ms` duplication after step 3
- [ ] Zero `PipelineError::Classified { run_error: RunError::` verbose constructions after step 1
- [ ] `run.rs` and `ci.rs` error handling blocks < 5 lines each after step 1
- [ ] `cargo test --workspace` green

---

## Risk Assessment

| Risk | Mitigation |
|------|-----------|
| Import churn breaks replay.rs | replay.rs uses `super::run_output::` not `super::pipeline::` — not affected |
| Tests break during move | Move tests with their functions, run after each step |
| Circular imports between pipeline_error.rs and reporting.rs | `pipeline_error.rs` only depends on `assay_core::errors::RunError` + `exit_codes`, not on reporting |
| Over-extraction | Step 4 has explicit decision gate — skip if duplication is small |

---

## Line Count Budget

| File | Before | After | Delta |
|------|--------|-------|-------|
| pipeline.rs | 557 | ~250 | -307 |
| pipeline_error.rs | — | ~50 | +50 |
| reporting.rs | — | ~300 | +300 |
| run.rs | 64 | ~40 | -24 |
| ci.rs | 207 | ~140 | -67 |
| profile.rs | 654 | 651 | -3 |
| **Net** | **1482** | **~1431** | **-51** |

Net line reduction is small — this is about separation of concerns, not line golf.
