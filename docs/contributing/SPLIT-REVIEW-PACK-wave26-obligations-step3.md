# SPLIT REVIEW PACK - Wave26 Obligations Step3

## Intent
Close Wave26 with a docs+gate-only Step3 slice after Step2 alert execution landed.

This slice must not:
- change MCP runtime behavior
- change CLI normalization behavior
- change MCP server behavior
- add high-risk obligations execution
- add external incident/case-management integration
- change workflow files

## Allowed files
- `docs/contributing/SPLIT-CHECKLIST-wave26-obligations-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave26-obligations-step3.md`
- `scripts/ci/review-wave26-obligations-step3.sh`

## What reviewers should verify
1. Diff is docs+script only.
2. Step3 reruns the same structural invariants from Step2.
3. Bounded executable scope remains `log` + `alert`.
4. `legacy_warning` compatibility remains intact.
5. `obligation_outcomes` remains additive.
6. No high-risk obligations execution markers appear.
7. No external incident/case-management integration markers appear.
8. Pinned tests still pass.

## Reviewer commands

### Against stacked Step2 base
```bash
BASE_REF=origin/codex/wave26-obligations-alert-step2-impl \
  bash scripts/ci/review-wave26-obligations-step3.sh
```

### Against origin/main after sync
```bash
BASE_REF=origin/main \
  bash scripts/ci/review-wave26-obligations-step3.sh
```
