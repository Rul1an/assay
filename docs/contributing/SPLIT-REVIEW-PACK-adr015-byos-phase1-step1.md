# SPLIT REVIEW PACK - ADR-015 BYOS Phase 1 Step1

## Intent

Freeze the bounded ADR-015 Phase 1 closure contract before any Step 2 implementation.

This slice is docs + gate only.

It must not:
- change evidence store runtime code
- change CLI command behavior
- change `EvalConfig` or the config loader
- add new crate dependencies
- touch workflow files

## Allowed files

- `docs/contributing/SPLIT-PLAN-adr015-byos-phase1-closure.md`
- `docs/contributing/SPLIT-CHECKLIST-adr015-byos-phase1-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-adr015-byos-phase1-step1.md`
- `scripts/ci/review-adr015-phase1-step1.sh`

## What reviewers should verify

1. Diff is limited to the four Step 1 files.
2. `store-status` command contract is explicit and bounded (args, output, exit codes).
3. Config design decision (option A, separate file) is explicit and does not touch `EvalConfig`.
4. Precedence chain is explicit: `--store` > env > config file > default lookup.
5. Provider docs scope is bounded (S3, B2, MinIO only in Phase 1).
6. Out-of-scope items are explicit (az/gcs, auto-push, Action integration, EvalConfig).
7. Step 2 frozen paths are explicit: `crates/assay-core/src/config.rs`, `crates/assay-core/src/model/types.rs`, `.github/workflows/*`.
8. No runtime code is touched.

## Reviewer command

```bash
BASE_REF=origin/main bash scripts/ci/review-adr015-phase1-step1.sh
```

Expected outcome:
- gate passes
- runtime code is untouched
- ADR-015 Phase 1 closure contract is frozen cleanly
- Step 2 can implement store-status, config, and docs without reopening design decisions
