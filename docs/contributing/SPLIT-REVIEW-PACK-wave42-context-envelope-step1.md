# SPLIT REVIEW PACK - Wave42 Context Envelope Step1

## Intent
Freeze the bounded decision context-envelope hardening contract before any Step2 implementation.

This slice is docs + gate only.

It must not:
- change MCP runtime behavior
- change CLI normalization behavior
- change MCP server behavior
- add new runtime capability
- touch workflow files

## Allowed files
- `docs/contributing/SPLIT-PLAN-wave42-context-envelope-hardening.md`
- `docs/contributing/SPLIT-CHECKLIST-wave42-context-envelope-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave42-context-envelope-step1.md`
- `scripts/ci/review-wave42-context-envelope-step1.sh`

## What reviewers should verify
1. Diff is limited to the four Step1 files.
2. Context payload surfaces are explicit (`DecisionEvent`, `DecisionData`).
3. Core context fields are explicit (`lane`, `principal`, `auth_context_summary`, `approval_state`).
4. Envelope completeness semantics are explicit and deterministic.
5. Runtime paths are untouched.
6. No runtime behavior change is introduced in this wave.

## Reviewer command
```bash
BASE_REF=origin/main bash scripts/ci/review-wave42-context-envelope-step1.sh
```

Expected outcome:
- gate passes
- runtime code is untouched
- context envelope contract is frozen cleanly
- Step2 can implement bounded context normalization without reopening semantics
