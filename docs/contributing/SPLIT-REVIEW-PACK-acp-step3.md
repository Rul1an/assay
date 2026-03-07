# ACP Step3 Review Pack (Closure)

## Intent

Close Wave14 ACP split with a docs+gate-only slice while preserving all Step2 invariants.

## Scope

- `docs/contributing/SPLIT-CHECKLIST-acp-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-acp-step3.md`
- `scripts/ci/review-acp-step3.sh`

## Non-goals

- no ACP code changes
- no workflow changes

## Validation

```bash
BASE_REF=origin/codex/wave14-acp-step2-mechanical bash scripts/ci/review-acp-step3.sh
```

## Reviewer 60s scan

1. Confirm Step3 diff is docs/script only.
2. Confirm Step3 gate re-runs Step2 quality + invariants.
3. Confirm no `.github/workflows/*` changes.
4. Run reviewer script and expect PASS.
