# Report: Split Refactor Program (Q1 2026)

Date: 2026-02-17
Verification basis:
- Mainline commit: `2e7c9758`
- Baseline snapshot commit (pre-program): `6ae1d340`
- Plan source: `docs/architecture/PLAN-split-refactor-2026q1.md`
- PR metadata source: `gh pr list` / `gh pr view`

## Executive summary

The split-refactor program is closed loop through Wave7C Step3 on `main`.
Evidence command:
- `gh pr view 377 --json number,state,mergedAt,baseRefName,headRefName,url`

Completed waves:
- Wave 0: guardrails
- Wave 1: verify/writer
- Wave 2: runner/mandate_store
- Wave 3: monitor/trace
- Wave 4: lockfile/cache/explain
- Wave 5: verify closure
- Wave 6: CI hardening closure (including Step4 readiness reporting)
- Wave 7: runtime/domain continuation through 7C Step3

Open non-program work at report time:
- PR #365 (docs auto-update)
- PR #376 (Dependabot uuid bump; checks in progress)

## Wave closure map

- Wave1 closure: PR #332 (writer split; verify closure tracked under Wave5)
- Wave2 closure: PR #336
- Wave3 closure: PR #337, #338
- Wave4 closure: PR #339, #340, #343, #344, #345
- Wave5 closure: PR #348, #349, #351
- Wave6 closure: PR #353, #355, #356, #359, #360, #362
- Wave7 closure: PR #363, #364, #366, #368, #369, #371, #377 (Wave7C Step3 closure)

## LOC outcomes (verified on main)

Baseline LOC in the table below is measured from the pre-program snapshot (`6ae1d340`).
Current LOC is measured on `main` at `2e7c9758`.

| File | Baseline LOC | Current LOC | Delta |
|---|---:|---:|---:|
| `crates/assay-evidence/src/bundle/writer.rs` | 1442 | 379 | -73.7% |
| `crates/assay-registry/src/verify.rs` | 1065 | 123 | -88.5% |
| `crates/assay-core/src/explain.rs` | 1057 | 11 | -99.0% |
| `crates/assay-core/src/runtime/mandate_store.rs` | 1046 | 748 | -28.5% |
| `crates/assay-core/src/engine/runner.rs` | 1042 | 661 | -36.6% |
| `crates/assay-core/src/providers/trace.rs` | 881 | 488 | -44.6% |
| `crates/assay-registry/src/lockfile.rs` | 863 | 649 | -24.8% |
| `crates/assay-registry/src/cache.rs` | 844 | 592 | -29.9% |
| `crates/assay-cli/src/cli/commands/monitor.rs` | 833 | 175 | -79.0% |
| `crates/assay-core/src/runtime/authorizer.rs` | 794 | 201 | -74.7% |
| `crates/assay-evidence/src/lint/packs/loader.rs` | 793 | 106 | -86.6% |
| `crates/assay-core/src/storage/store.rs` | 774 | 658 | -15.0% |
| `crates/assay-core/src/judge/mod.rs` | 712 | 71 | -90.0% |
| `crates/assay-evidence/src/json_strict/mod.rs` | 759 | 81 | -89.3% |

## Current largest production Rust files (main snapshot)

(Excluding generated files and test modules)
(Filter note: scan excludes `*/tests/*` and `*tests.rs`, but may still count `#[cfg(test)]` blocks inside production files.)

1. `crates/assay-cli/src/env_filter.rs` (767)
2. `crates/assay-core/src/runtime/mandate_store.rs` (748)
3. `crates/assay-core/src/agentic/mod.rs` (742)
4. `crates/assay-cli/src/cli/args/mod.rs` (739)
5. `crates/assay-cli/src/cli/commands/replay.rs` (734)
6. `crates/assay-core/src/model.rs` (726)
7. `crates/assay-evidence/src/mandate/types.rs` (715)
8. `crates/assay-core/src/replay/bundle.rs` (705)
9. `crates/assay-registry/src/auth.rs` (685)
10. `crates/assay-registry/src/trust.rs` (664)

No production file in this set is above 800 LOC.

## Risk and follow-up notes

- Program closure is complete for waves 1-7C; remaining work is maintenance and optional hardening waves.
- Plan file had stale status text and has been synchronized.
- Some older wave docs still contain absolute local paths in reviewer artifacts; this is cosmetic but should be normalized if touched again.
- Follow-up hygiene suggestion: add a lightweight docs check that rejects new absolute `/Users/...` paths in reviewer artifacts.
