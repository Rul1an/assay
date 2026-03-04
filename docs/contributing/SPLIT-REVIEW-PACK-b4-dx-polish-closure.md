# SPLIT REVIEW PACK — B4 DX Polish Closure (C-slice)

## Intent
Close B4 with docs-only operational guidance and a strict reviewer gate.

## Scope
- `/Users/roelschuurkes/assay/docs/ops/COVERAGE-AND-WRAP-DX-RUNBOOK.md`
- `/Users/roelschuurkes/assay/docs/contributing/SPLIT-CHECKLIST-b4-dx-polish-closure.md`
- `/Users/roelschuurkes/assay/docs/contributing/SPLIT-REVIEW-PACK-b4-dx-polish-closure.md`
- `/Users/roelschuurkes/assay/scripts/ci/review-b4c-dx-closure.sh`

## Safety
- Docs + gate only
- No workflows
- No runtime/schema changes

## Reviewer Quick Check
1. Confirm runbook contains:
- `assay coverage --format md`
- `--declared-tools-file`
- `assay mcp wrap --coverage-out`
- `assay mcp wrap --state-window-out`
- `wrapped > coverage > state-window`

2. Run:
```bash
BASE_REF=origin/main bash scripts/ci/review-b4c-dx-closure.sh
```

Expected:
- Gate passes
- Docs are sufficient for new contributors to use B4 DX outputs consistently.
