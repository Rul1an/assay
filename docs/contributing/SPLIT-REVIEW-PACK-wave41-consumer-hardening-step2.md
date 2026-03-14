# SPLIT REVIEW PACK - Wave41 Consumer Hardening Step2

## Intent
Implement bounded consumer/read precedence hardening with additive compatibility metadata for decision and replay payload consumers.

This slice must:
- remain additive and backward-compatible
- expose deterministic consumer read precedence
- signal consumer-facing fallback explicitly
- keep runtime behavior unchanged

This slice must not:
- add new runtime capability
- change enforcement semantics
- expand policy-engine/control-plane/auth transport scope
- touch workflow files

## Reviewer focus
1. Diff stays inside bounded decision/replay/test/docs/gate scope.
2. Consumer-hardening fields are present and additive in event payload + replay basis.
3. Consumer read precedence is deterministic and explicit.
4. Consumer fallback signaling remains backward-compatible.
5. No scope creep into runtime behavior changes.

## Reviewer command
```bash
BASE_REF=origin/main bash scripts/ci/review-wave41-consumer-hardening-step2.sh
```
