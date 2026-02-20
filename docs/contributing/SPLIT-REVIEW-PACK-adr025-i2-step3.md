# ADR-025 I2 Step3 Review Pack (closure rollout informational)

## Summary
Adds an informational nightly closure lane that evaluates closure from:
- fresh soak report (`assay sim soak`)
- latest nightly readiness artifact (`adr025-nightly-readiness`)
- I2 manifest fixture (`scripts/ci/fixtures/adr025-i2/manifest_good.json`)
- closure policy (`schemas/closure_policy_v1.json`)

Outputs are uploaded as artifact `adr025-closure-report`.

## Safety contracts
- Schedule + dispatch only (no PR triggers)
- Job-level `continue-on-error: true`
- SHA-pinned actions only
- Minimal permissions only
- No required-check / branch-protection changes

## Verification
- `BASE_REF=origin/main bash scripts/ci/review-adr025-i2-step3.sh`
- Workflow contains evaluator invocation: `scripts/ci/adr025-closure-evaluate.sh`
- Artifact contract:
  - `closure_report_v1.json`
  - `closure_report_v1.md`

## Notes
- This is informational-only rollout (I2 Step3).
- Enforcement decisions remain outside this slice.
