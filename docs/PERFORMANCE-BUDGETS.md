# Performance Budgets (Wave C Harness)

This document defines the reproducible workload classes and baseline budgets used to gate Wave C optimization work.

## Workload Classes

| Class | Bundle Size Target | Event Count | Rule Count Target | Usage |
|------|---------------------|-------------|-------------------|-------|
| `small` | ~1 MB | 1k | ~10 | Fast local smoke/perf sanity |
| `typical-pr` | ~10 MB | 10k | ~50 | Default CI-level perf guardrail |
| `large` | 50 MB+ | 100k+ | 500+ | Scale trigger for C1/C3/C4 |

Bundle size targets are **logical payload targets** (uncompressed event content). The harness uses
deterministic low-compressibility payloads so compressed tar sizes do not collapse unrealistically.

## Harness Commands

Default (`small` + `typical-pr`):

```bash
cargo bench -p assay-evidence --bench verify_lint_harness
```

Single class (example: `large`):

```bash
ASSAY_PERF_WORKLOAD=large cargo bench -p assay-evidence --bench verify_lint_harness
```

All classes:

```bash
ASSAY_PERF_WORKLOAD=small,typical-pr,large cargo bench -p assay-evidence --bench verify_lint_harness
```

Profile-store harness (C3):

```bash
cargo bench -p assay-cli --bench profile_store_harness
```

Profile-store single class:

```bash
ASSAY_PROFILE_PERF_WORKLOAD=large cargo bench -p assay-cli --bench profile_store_harness
```

## Trigger Budgets (Ubuntu Baseline)

These are trigger thresholds, not pass/fail release gates.

The harness emits `verify/*`, `lint/*`, and `verify+lint/*` series per workload. Trigger checks for C1
must use the explicit `verify+lint/*` series from the same Criterion run.

Measurement protocol (to keep comparisons stable):
- Runner: `ubuntu-latest` as baseline.
- Percentiles: use both `p50` and `p95`.
- Warm/cold split:
  - cold = first run after clean build/artifact state
  - warm = repeated runs on same runner/workdir
  - trigger decisions use warm `p95` and cold `p50` together when relevant.

- C1 trigger:
  - verify+lint `p95 > 5s` on `large`
  - or verify+lint `p50 > 2s` on `typical-pr`
- C2 trigger:
  - runner clone/build overhead > 10% of suite runtime on >=1000 tests
- C3 trigger:
  - profile merge `p95 > 1s` at >=10k entries (`profile/merge/typical-pr` or higher)
  - or profile load `p95 > 500ms` (`profile/load/typical-pr` or higher)
- C4 trigger:
  - run-id tracking evictions cause determinism or duplicate-merge issues
  - hard bound for duplicate protection window: `N = 5000` recent run IDs

## Guardrails

- No semantic changes to verify/lint/run outputs in Wave C.
- Any optimization PR must include before/after benchmark output from this harness.
- Golden equivalence tests are required for verify/lint behavior changes.
