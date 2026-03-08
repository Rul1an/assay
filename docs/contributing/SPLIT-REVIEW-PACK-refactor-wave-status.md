# SPLIT REVIEW PACK — Refactor Wave Status closure

## Intent
Close the refactor-wave line with a single operational status page.

This slice is docs+gate only.
It does not change runtime behavior, CI behavior, or any code path.

## Allowed files
- `docs/contributing/REFACTOR-WAVE-STATUS.md`
- `docs/contributing/SPLIT-CHECKLIST-refactor-wave-status.md`
- `docs/contributing/SPLIT-REVIEW-PACK-refactor-wave-status.md`
- `scripts/ci/review-refactor-wave-status.sh`

## Reviewer expectations
Reviewers should verify:

1. the diff is docs+script only
2. no workflow files are touched
3. the listed waves are actually closed-loop on `main`
4. the standing refactor policy matches the current repo practice

## Reviewer command
```bash
BASE_REF=origin/main bash scripts/ci/review-refactor-wave-status.sh
```

## Expected outcome
- gate passes
- status page is accurate
- future wave planning starts from current `main`, not stale hotspot lists
