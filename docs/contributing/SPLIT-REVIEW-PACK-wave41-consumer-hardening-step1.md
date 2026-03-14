# SPLIT REVIEW PACK - Wave41 Consumer Hardening Step1

## Intent
Freeze the bounded decision/replay consumer hardening contract before any Step2 implementation.

This slice is docs + gate only.

It must not:
- change MCP runtime behavior
- change CLI normalization behavior
- change MCP server behavior
- add new runtime capability
- touch workflow files

## Allowed files
- `docs/contributing/SPLIT-PLAN-wave41-consumer-hardening.md`
- `docs/contributing/SPLIT-CHECKLIST-wave41-consumer-hardening-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave41-consumer-hardening-step1.md`
- `scripts/ci/review-wave41-consumer-hardening-step1.sh`

## What reviewers should verify
1. Diff is limited to the four Step1 files.
2. Consumer payload surfaces are explicit (`DecisionEvent`, `DecisionData`, `ReplayDiffBasis`).
3. Consumer read precedence is explicit and deterministic.
4. Additive consumer compatibility expectations are explicit.
5. Runtime paths are untouched.
6. No runtime behavior change is introduced in this wave.

## Reviewer command
```bash
BASE_REF=origin/main bash scripts/ci/review-wave41-consumer-hardening-step1.sh
```

Expected outcome:
- gate passes
- runtime code is untouched
- consumer hardening contract is frozen cleanly
- Step2 can implement bounded consumer normalization without reopening semantics
