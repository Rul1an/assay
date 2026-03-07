# Model Step3 Review Pack (Closure)

## Intent

Close Wave13 model split with strict closure gating and promote-safe checks.

## Scope

- `docs/contributing/SPLIT-CHECKLIST-model-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-model-step3.md`
- `scripts/ci/review-model-step3.sh`

## Non-goals

- no model code changes in Step3
- no workflow changes
- no API or behavior changes

## Validation

```bash
BASE_REF=origin/codex/wave13-model-step2-mechanical bash scripts/ci/review-model-step3.sh
BASE_REF=origin/main bash scripts/ci/review-model-step3.sh
```

## Reviewer 60s scan

1. Confirm Step3 diff is docs/script-only.
2. Confirm Step2 invariants are still green.
3. Confirm promote-precheck mode (`BASE_REF=origin/main`) passes.
