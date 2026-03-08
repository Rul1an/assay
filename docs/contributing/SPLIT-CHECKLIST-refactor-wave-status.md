# SPLIT CHECKLIST — Refactor Wave Status closure

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/REFACTOR-WAVE-STATUS.md`
  - `docs/contributing/SPLIT-CHECKLIST-refactor-wave-status.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-refactor-wave-status.md`
  - `scripts/ci/review-refactor-wave-status.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No code changes
- [ ] No changelog / release-note drift

## Content checks
- [ ] Closed-loop waves listed are actually on `main`
- [ ] Status text does not claim unfinished waves are complete
- [ ] Standing refactor policy matches current practice
- [ ] "Definition of done" matches current merge discipline
- [ ] Document is brief and operational, not narrative

## Reviewer gate
- [ ] Allowlist-only
- [ ] Workflow-ban
- [ ] Marker checks for title and key sections
