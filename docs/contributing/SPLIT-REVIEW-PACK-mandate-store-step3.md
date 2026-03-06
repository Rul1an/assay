# Mandate Store Step 3 Review Pack (Closure)

## Intent

Close the mandate-store split with a strict, docs-only closure step and final guardrails.

## Scope

- `docs/contributing/SPLIT-CHECKLIST-mandate-store-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-mandate-store-step3.md`
- `scripts/ci/review-mandate-store-step3.sh`

## Final shape snapshot

| File | Step1 LOC | Step2 LOC | Delta |
| --- | ---: | ---: | ---: |
| `crates/assay-core/src/runtime/mandate_store.rs` | 748 | 195 | -553 |
| `crates/assay-core/src/runtime/mandate_store_next/tests.rs` | 3 | 553 | +550 |
| `crates/assay-core/src/runtime/mandate_store_next/mod.rs` | 12 | 11 | -1 |

## Non-goals

- No production code edits.
- No behavior changes.
- No workflow changes.

## Validation command

```bash
BASE_REF=<step2-base> bash scripts/ci/review-mandate-store-step3.sh
```

## Reviewer 60s scan

1. Confirm Step3 diff only touches docs + Step3 reviewer script.
2. Run Step3 reviewer script and confirm PASS.
3. Verify final layout invariants (thin facade, externalized tests, transaction boundary single-source).
