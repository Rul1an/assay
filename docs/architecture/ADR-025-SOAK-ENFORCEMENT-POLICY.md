# ADR-025 Soak Enforcement Policy (v1)

## Intent
Enable fail-closed enforcement for releases/promotions based on readiness signals, while keeping PR lanes unchanged.

## Scope
- Enforced only in release/promote workflows.
- Nightly soak + readiness remain informational.
- No pull_request triggers introduced.
- No required-check changes for PRs.

## Inputs
Readiness artifact (from Step3 C2):
- Artifact name: `adr025-nightly-readiness`
- Files:
  - `nightly_readiness.json`
  - `nightly_readiness.md`

## Policy contract (v1)
### Window
- Default: last 20 runs observed.
- Minimum required runs: 14
- If runs_observed < minimum: treat as **measurement/contract failure**.

### Thresholds
- success_rate >= 0.90
- contract_fail_rate <= 0.05
- infra_fail_rate <= 0.01
- unknown_rate <= 0.05

### Exit codes (enforcement script)
- 0: pass (eligible to promote)
- 1: policy fail (thresholds violated)
- 2: measurement/contract fail (missing artifact, invalid schema, insufficient window, parse errors)

## Determinism / Reproducibility
- Enforcement evaluation must be reproducible from:
  - readiness artifact JSON
  - policy file (v1)
  - script version

## Override / break-glass
- Overrides must be explicit and auditable (release owner action).
- No silent bypass in PR lanes.
- Override mechanism is defined in Step4C runbook (not in this policy doc).
