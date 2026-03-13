# SPLIT REVIEW PACK — Wave33 Obligation Outcomes Step2

## Intent
Implement additive normalization fields for `obligation_outcomes` without changing runtime decision behavior.

## Allowed implementation surface
- `assay-core` MCP decision/outcome emission paths
- `assay-core` tests for normalization invariants
- Step2 docs/gate files

## What reviewers should verify
1. Diff is bounded to normalization fields + tests + Step2 docs/gate.
2. `ObligationOutcome` includes additive fields:
   - `reason_code`
   - `enforcement_stage`
   - `normalization_version`
3. Existing fields remain intact:
   - `obligation_type`
   - `status`
   - `reason`
4. Legacy and handler paths emit deterministic reason codes.
5. Existing allow/deny behavior remains unchanged.
6. No scope creep into new obligation semantics.

## Reviewer command
```bash
BASE_REF=origin/codex/wave33-obligation-outcomes-step1-freeze bash scripts/ci/review-wave33-obligation-outcomes-step2.sh
```
