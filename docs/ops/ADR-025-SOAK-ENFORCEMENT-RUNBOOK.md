# ADR-025 Soak Enforcement Runbook (Step4C)

## Purpose
Operate release-lane soak enforcement safely and reproducibly, without impacting PR lanes.

## Scope
- Applies to `.github/workflows/release.yml` only.
- Does not change PR required checks.
- Nightly readiness generation remains informational.

## Enforcement flow
1. Release job fetches latest successful `adr025-nightly-readiness` run on `main`.
2. Downloads artifact `adr025-nightly-readiness`.
3. Requires `input/readiness/nightly_readiness.json` to exist.
4. Runs `scripts/ci/adr025-soak-enforce.sh` against:
   - `schemas/soak_readiness_policy_v1.json`
   - downloaded readiness JSON

## Exit contract
- `0`: pass (release can continue)
- `1`: policy fail (threshold violation)
- `2`: measurement/contract fail (missing artifact/json, parse error, classifier mismatch, insufficient window)

## Fail-closed behavior
Any non-zero result from enforcement blocks release publishing in release lane.

## Policy invariants (v1)
- classifier lock: readiness `classifier_version` must equal policy `classifier_version`.
- minimum window: `window.runs_observed >= window.runs_observed_minimum`.
- thresholds:
  - success_rate >= 0.90
  - contract_fail_rate <= 0.05
  - infra_fail_rate <= 0.01
  - unknown_rate <= 0.05

## Operator triage
When release blocks:
1. Inspect release logs for enforcement step output.
2. Identify failure class:
   - policy fail (`exit 1`) -> readiness below thresholds
   - measurement/contract fail (`exit 2`) -> artifact/schema/window/classifier issue
3. Confirm latest readiness artifact exists and is valid.
4. Re-run readiness workflow manually if needed (`workflow_dispatch`) and retry release.

## Break-glass (controlled)
- Allowed only by release owner.
- Must be explicit, auditable, and temporary.
- No silent bypass in PR lanes.
- Any break-glass action requires:
  - reason logged in release run notes/comment
  - follow-up issue to restore normal enforcement if disabled

## Evidence to capture in incidents
- release workflow run URL
- readiness workflow run URL used by enforcement
- enforcement script output and exit code
- policy version and classifier version values
