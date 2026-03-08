# Mandate Types Step3 Review Pack (Closure)

## Intent

Close Wave18 mandate-types split with strict closure gating and promote-safe checks.

## Scope

- `docs/contributing/SPLIT-CHECKLIST-mandate-types-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-mandate-types-step3.md`
- `scripts/ci/review-mandate-types-step3.sh`

## Non-goals

- no `crates/assay-evidence/src/mandate/types/**` changes in Step3
- no workflow changes
- no API or behavior changes

## Validation

```bash
BASE_REF=origin/codex/wave18-mandate-types-step2-mechanical bash scripts/ci/review-mandate-types-step3.sh
BASE_REF=origin/main bash scripts/ci/review-mandate-types-step3.sh
```

## Reviewer 60s scan

1. Confirm Step3 diff is docs/script-only.
2. Confirm Step2 invariants still pass in Step3 gate.
3. Confirm promote-precheck mode (`BASE_REF=origin/main`) passes.
