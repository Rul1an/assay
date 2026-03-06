# Wave8B Step3 Review Pack - UCP Closure

## Intent

Close Wave8B with a final reviewer gate and closure docs after the mechanical split landed.

## Scope

- `docs/contributing/SPLIT-CHECKLIST-wave8b-step3-ucp.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave8b-step3-ucp.md`
- `scripts/ci/review-wave8b-step3.sh`

## Non-goals

- No code movement in this step
- No new behavior changes
- No workflow changes

## Validation command

```bash
BASE_REF=origin/main bash scripts/ci/review-wave8b-step3.sh
```
