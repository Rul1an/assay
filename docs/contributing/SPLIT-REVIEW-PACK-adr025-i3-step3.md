# ADR-025 I3 Step3 Review Pack (OTel bridge rollout informational)

## Summary
Adds an informational nightly OTel bridge lane that generates `otel_bridge_report_v1` from deterministic fixture input:
- Generator: `scripts/ci/adr025-otel-bridge.sh`
- Input fixture: `scripts/ci/fixtures/adr025-i3/otel_input_minimal.json`
- Output artifact: `adr025-otel-bridge-report` (retention: 14 days)

Stabilization status (on main):
- I3 Stab A: hardening contract frozen (`scripts/ci/review-adr025-i3-stab-a.sh`)
- I3 Stab B: determinism edge-case fixtures + invariant assertions (`scripts/ci/review-adr025-i3-stab-b.sh`)

## Safety contracts
- Schedule + dispatch only (no PR triggers)
- Job-level `continue-on-error: true`
- SHA-pinned actions only
- Minimal permissions only
- No required-check / branch-protection changes

## Verification
- Reviewer gate: `BASE_REF=origin/main bash scripts/ci/review-adr025-i3-step3.sh`
- Determinism tests: `bash scripts/ci/test-adr025-otel-bridge.sh`
- Workflow contains generator invocation: `scripts/ci/adr025-otel-bridge.sh`
- Artifact contract:
  - `otel_bridge_report_v1.json`
  - `otel_bridge_report_v1.md`

## Notes
- This is informational-only rollout (I3 Step3).
- Enforcement decisions remain outside this slice.
