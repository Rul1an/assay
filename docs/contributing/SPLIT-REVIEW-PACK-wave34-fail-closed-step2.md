# SPLIT REVIEW PACK - Wave34 Fail-Closed Step2

## Intent
Implement bounded fail-closed matrix typing with additive decision evidence.

This slice must:
- keep behavior deterministic
- preserve backward-compatible event shape
- avoid scope creep beyond fail-closed typing

This slice must not:
- add new obligation types
- redesign auth transport
- add control-plane workflow semantics
- add external incident/case integrations
- touch workflow files

## Reviewer focus
1. Diff stays within bounded runtime/test/docs/gate scope.
2. `FailClosedContext` is additive and typed.
3. Matrix dimensions are present and deterministic.
4. Baseline fail-closed reason codes are present.
5. Existing obligation execution and reason-code behavior remains stable.
6. No scope creep into non-goals.

## Reviewer command
```bash
BASE_REF=origin/codex/wave34-fail-closed-matrix-step1-freeze \
  bash scripts/ci/review-wave34-fail-closed-step2.sh
```
