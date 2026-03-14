# SPLIT REVIEW PACK - Wave39 Evidence Compat Step2

## Intent
Implement bounded replay/evidence compatibility normalization fields and deterministic classification precedence.

This slice must:
- remain additive and backward-compatible
- normalize legacy fallback metadata deterministically
- keep runtime behavior unchanged

This slice must not:
- add new runtime capability
- change enforcement semantics
- expand policy-engine/control-plane/auth transport scope
- touch workflow files

## Reviewer focus
1. Diff stays inside bounded runtime/test/docs/gate scope.
2. Compatibility fields are present and additive in decision payload + replay basis.
3. Classification precedence is deterministic (converged > fulfillment > legacy).
4. Legacy fallback signaling remains backward-compatible.
5. No scope creep into non-goals.

## Reviewer command
```bash
BASE_REF=origin/main bash scripts/ci/review-wave39-evidence-compat-step2.sh
```
