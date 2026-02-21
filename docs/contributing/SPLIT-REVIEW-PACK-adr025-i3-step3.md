# ADR-025 I3 Step3 Review Pack (OTel bridge rollout informational)

## Summary
Adds an informational nightly OTel bridge lane that generates `otel_bridge_report_v1` from deterministic fixture input:
- generator: `scripts/ci/adr025-otel-bridge.sh`
- input fixture: `scripts/ci/fixtures/adr025-i3/otel_input_minimal.json`
- output artifact: `adr025-otel-bridge-report`

## Safety contracts
- Schedule + dispatch only (no PR triggers)
- Job-level `continue-on-error: true`
- SHA-pinned actions only
- Minimal permissions only
- No required-check / branch-protection changes

## Verification
- `BASE_REF=origin/main bash scripts/ci/review-adr025-i3-step3.sh`
- Workflow contains generator invocation: `scripts/ci/adr025-otel-bridge.sh`
- Artifact contract:
  - `otel_bridge_report_v1.json`
  - `otel_bridge_report_v1.md`

## Notes
- This is informational-only rollout (I3 Step3).
- Enforcement decisions remain outside this slice.
