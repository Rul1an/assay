# SPLIT REVIEW PACK — Coverage v1.1 Closure (C-slice)

## Intent
Close the Coverage v1.1 DX line with operational docs and a reviewer gate.

## Scope
Docs + gate only:
- `docs/ops/COVERAGE-V1-1-RUNBOOK.md`
- `docs/contributing/SPLIT-CHECKLIST-coverage-v1-1-closure.md`
- `docs/contributing/SPLIT-REVIEW-PACK-coverage-v1-1-closure.md`
- `scripts/ci/review-coverage-v1-1-c-closure.sh`

## Safety
- No runtime changes
- No workflows
- No schema/ADR changes

## Reviewer quick-check (60s)
1) Confirm the runbook includes:
- `--out-md`
- `--routes-top`
- canonical JSON vs derived markdown
- exit codes 0/2/3
2) Run the reviewer gate:

```bash
BASE_REF=origin/main bash scripts/ci/review-coverage-v1-1-c-closure.sh
```

Expected outcome:
- Gate passes
- New dev can run Coverage v1.1 without reading code
