# SPLIT REVIEW PACK - Wave40 Deny Evidence Step2

## Intent
Implement bounded deny/fail-closed evidence convergence with deterministic precedence and additive legacy fallback metadata.

This slice must:
- remain additive and backward-compatible
- separate deny classes explicitly in emitted evidence
- represent deterministic deny precedence
- keep runtime behavior unchanged

This slice must not:
- add new runtime capability
- change enforcement semantics
- expand policy-engine/control-plane/auth transport scope
- touch workflow files

## Reviewer focus
1. Diff stays inside bounded decision/runtime/test/docs/gate scope.
2. Deny classification fields are present and additive in event payload + replay basis.
3. Deny precedence is deterministic (outcome > origin > fulfillment > legacy).
4. Legacy deny fallback signaling remains backward-compatible.
5. No scope creep into runtime behavior changes.

## Reviewer command
```bash
BASE_REF=origin/main bash scripts/ci/review-wave40-deny-evidence-step2.sh
```
