# SPLIT REVIEW PACK — Wave24 Typed Decisions Step3

## Intent
Close Wave24 with a docs+gate-only closure slice after the typed decision and Decision Event v2 implementation.

This slice must not:
- change MCP runtime behavior
- change CLI normalization behavior
- change MCP server auth/runtime behavior
- add obligations execution
- add approval enforcement
- touch workflow files

## Allowed files
- `docs/contributing/SPLIT-CHECKLIST-typed-decisions-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-typed-decisions-step3.md`
- `scripts/ci/review-wave24-typed-decisions-step3.sh`

## What reviewers should verify
1. Diff is docs+script only.
2. Step3 reruns the same structural invariants from Step2.
3. Typed decision markers remain present:
   - `allow_with_obligations`
   - `deny_with_alert`
4. `AllowWithWarning` compatibility remains intact.
5. Decision Event v2 field markers remain present.
6. Existing event fields remain present.
7. No obligations execution markers appear in runtime scope.
8. Pinned tests still pass.

## Reviewer commands

### Against stacked Step2 base
```bash
BASE_REF=origin/codex/wave24-typed-decisions-step2-impl \
  bash scripts/ci/review-wave24-typed-decisions-step3.sh
```

### Against `origin/main` after sync
```bash
BASE_REF=origin/main \
  bash scripts/ci/review-wave24-typed-decisions-step3.sh
```

Expected outcome:
- Step3 adds no runtime behavior
- closure remains diff-proof
- promote can happen cleanly after stacked validation
