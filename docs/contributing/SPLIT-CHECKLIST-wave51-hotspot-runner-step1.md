# SPLIT CHECKLIST - Wave 51 Runner Step1

## Scope Lock

- Move implementation only for `engine::runner`.
- Keep `Runner::run_suite` and existing private method call sites stable.
- No behavior changes to retry, cache, metric, baseline, judge, embedding, progress, or assertion semantics.
- No workflow edits.
- No generated file edits.

## Files

- `crates/assay-core/src/engine/runner.rs`
- `crates/assay-core/src/engine/runner_next/mod.rs`
- `crates/assay-core/src/engine/runner_next/assertions.rs`
- `crates/assay-core/src/engine/runner_next/single.rs`
- `docs/contributing/SPLIT-PLAN-wave51-hotspot-refactor-2026q2.md`
- `docs/contributing/SPLIT-CHECKLIST-wave51-hotspot-runner-step1.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave51-hotspot-runner-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave51-hotspot-runner-step1.md`
- `scripts/ci/review-wave51-hotspot-runner-step1.sh`

## Drift Gates

- `runner.rs` non-test code stays under 140 lines.
- `runner.rs` delegates to `runner_next::assertions::apply_agent_assertions_impl`.
- `runner.rs` delegates to `runner_next::single::run_test_once_impl`.
- `runner_next/single.rs` owns the metric span loop and cache key logic.
- `runner_next/assertions.rs` owns assertion overlay status/message/details mutation.

## Validation

```bash
cargo fmt --check
cargo check -p assay-core
cargo test -p assay-core --lib runner_contract_
bash scripts/ci/review-wave51-hotspot-runner-step1.sh
```

## Definition of Done

- Step 1 review script passes.
- Existing runner contract tests pass.
- LOC delta is reported in the review pack.
- Next hotspot remains untouched.
