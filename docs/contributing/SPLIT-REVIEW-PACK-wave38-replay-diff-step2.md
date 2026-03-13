# SPLIT REVIEW PACK - Wave38 Replay Diff Step2

## Intent
Implement bounded replay/diff contract primitives for deterministic evidence comparison.

This slice must:
- keep runtime behavior unchanged
- add typed replay basis + diff buckets
- keep changes additive and backward-compatible

This slice must not:
- add new runtime capabilities
- expand policy-engine scope
- add UI/control-plane/auth transport work
- touch workflow files

## Reviewer focus
1. Diff stays inside bounded runtime/test/docs/gate scope.
2. Replay basis fields are present and deterministic.
3. Diff bucket classifier is deterministic and additive.
4. Existing runtime behavior remains unchanged.
5. No scope creep into non-goals.

## Reviewer command
```bash
BASE_REF=origin/main bash scripts/ci/review-wave38-replay-diff-step2.sh
```
