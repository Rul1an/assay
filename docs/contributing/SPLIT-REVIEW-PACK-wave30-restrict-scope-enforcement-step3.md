# SPLIT REVIEW PACK — Wave30 Restrict Scope Enforcement Step3

## Intent
Close Wave30 with a docs+gate-only closure slice after bounded `restrict_scope` runtime enforcement landed in Step2.

This slice must not:
- change MCP runtime behavior
- change CLI normalization behavior
- change MCP server behavior
- add rewrite/filter/redact behavior
- add broad/global scope semantics
- touch workflow files

## Allowed files
- `docs/contributing/SPLIT-CHECKLIST-wave30-restrict-scope-enforcement-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave30-restrict-scope-enforcement-step3.md`
- `scripts/ci/review-wave30-restrict-scope-enforcement-step3.sh`

## What reviewers should verify
1. Diff is docs+script only.
2. Step3 reruns the same structural invariants from Step2.
3. `restrict_scope` runtime enforcement markers remain present.
4. Deterministic deny behavior for scope validation failures remains present.
5. Scope evidence remains additive and backward-compatible.
6. Existing `log`/`alert`/`approval_required` line remains intact.
7. No scope creep appears in runtime scope.
8. Pinned tests still pass.

## Reviewer commands

### Against stacked Step2 base
```bash
BASE_REF=origin/codex/wave30-restrict-scope-step2-impl \
  bash scripts/ci/review-wave30-restrict-scope-enforcement-step3.sh
```

### Against origin/main after sync
```bash
BASE_REF=origin/main \
  bash scripts/ci/review-wave30-restrict-scope-enforcement-step3.sh
```

## Expected outcome
- Step3 adds no runtime behavior.
- Closure remains diff-proof.
- Promote can happen cleanly after stacked validation.
