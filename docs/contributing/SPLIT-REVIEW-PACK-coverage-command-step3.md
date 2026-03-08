# Coverage Command Step3 Review Pack (Closure)

## Intent

Close Wave19 coverage-command split with a docs+gate-only slice while re-running Step2 invariants and contract checks.

## Scope

- `docs/contributing/SPLIT-CHECKLIST-coverage-command-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-coverage-command-step3.md`
- `scripts/ci/review-coverage-command-step3.sh`

## Non-goals

- no code changes under `crates/assay-cli/src/cli/commands/coverage/**`
- no workflow changes
- no behavior or exit-code changes

## Validation

Stacked base:

```bash
BASE_REF=origin/codex/wave19-coverage-command-step2-mechanical bash scripts/ci/review-coverage-command-step3.sh
```

Promote sanity:

```bash
BASE_REF=origin/main bash scripts/ci/review-coverage-command-step3.sh
```

## Reviewer 60s scan

1. Confirm diff is only the 3 Step3 files.
2. Confirm workflow-ban exists in the script.
3. Confirm Step2 invariants are re-run in the script.
4. Confirm coverage contract tests are re-run.
