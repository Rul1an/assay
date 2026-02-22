# ADR-025 Index

## Intent
Single entry point for ADR-025 deliverables across I1/I2/I3:
- where to start reading
- where schemas/scripts/workflows live
- how to validate locally
- what is informational vs release-lane evidence

## Quick Start (Where to look first)
1. Product-level ADR:
   - `docs/architecture/ADR-025-Evidence-as-a-Product.md`
2. Iteration plans:
   - `docs/architecture/PLAN-ADR-025-I1-audit-kit-soak-2026q2.md`
   - `docs/architecture/PLAN-ADR-025-I2-audit-kit-closure-2026q2.md`
   - `docs/architecture/PLAN-ADR-025-I3-otel-bridge-2026q2.md`
3. Release integration contracts:
   - `docs/architecture/ADR-025-SOAK-ENFORCEMENT-POLICY.md`
   - `docs/architecture/ADR-025-I2-CLOSURE-RELEASE-INTEGRATION.md`
   - `docs/architecture/ADR-025-I3-OTEL-RELEASE-INTEGRATION.md`
4. Ops runbooks:
   - `docs/ops/ADR-025-SOAK-ENFORCEMENT-RUNBOOK.md`
   - `docs/ops/ADR-025-I2-CLOSURE-RELEASE-RUNBOOK.md`
   - `docs/ops/ADR-025-I3-OTEL-BRIDGE-RUNBOOK.md`
   - `docs/ops/ADR-025-I3-OTEL-RELEASE-RUNBOOK.md`

## Local Validation (Common)
- `bash scripts/ci/test-adr025-soak-enforce.sh`
- `bash scripts/ci/test-adr025-closure-evaluate.sh`
- `bash scripts/ci/test-adr025-closure-release.sh`
- `bash scripts/ci/test-adr025-otel-bridge.sh`
- `bash scripts/ci/test-adr025-otel-release.sh`

## Iterations Overview

### I1 - Soak + readiness + release enforcement
Primary outputs:
- informational soak/readiness lanes
- fail-closed readiness enforcement in release lane

Key files:
- schemas: `schemas/soak_readiness_policy_v1.json`
- scripts:
  - `scripts/ci/adr025-soak-readiness-report.sh`
  - `scripts/ci/adr025-soak-enforce.sh`
- workflows:
  - `.github/workflows/adr025-nightly-soak.yml`
  - `.github/workflows/adr025-nightly-readiness.yml`
  - `.github/workflows/release.yml` (readiness enforcement step)

### I2 - Closure (completeness/provenance) + release attach
Primary outputs:
- closure report generation + informational nightly closure lane
- release-lane closure evidence attach/enforce modes

Key files:
- schemas:
  - `schemas/closure_report_v1.schema.json`
  - `schemas/closure_policy_v1.json`
  - `schemas/closure_release_policy_v1.json`
- scripts:
  - `scripts/ci/adr025-closure-evaluate.sh`
  - `scripts/ci/adr025-closure-release.sh`
- workflows:
  - `.github/workflows/adr025-nightly-closure.yml`
  - `.github/workflows/release.yml` (closure integration step)

### I3 - OTel bridge + release attach
Primary outputs:
- OTel bridge report generator + informational nightly OTel lane
- release-lane OTel evidence attach/enforce modes

Key files:
- schemas:
  - `schemas/otel_bridge_report_v1.schema.json`
  - `schemas/otel_release_policy_v1.json`
- scripts:
  - `scripts/ci/adr025-otel-bridge.sh`
  - `scripts/ci/adr025-otel-release.sh`
- workflows:
  - `.github/workflows/adr025-nightly-otel-bridge.yml`
  - `.github/workflows/release.yml` (OTel integration step)

## Artifacts Map

| Iteration | Artifact | Produced By | Contract |
|---|---|---|---|
| I1 | `adr025-soak-report` | `.github/workflows/adr025-nightly-soak.yml` | soak report JSON + retention 14 days |
| I1 | `adr025-nightly-readiness` | `.github/workflows/adr025-nightly-readiness.yml` | `nightly_readiness.json` + `nightly_readiness.md`, retention 14 days |
| I2 | `adr025-closure-report` | `.github/workflows/adr025-nightly-closure.yml` | `closure_report_v1.json` + `closure_report_v1.md`, retention 14 days |
| I2 | `adr025-closure-release-evidence` | `.github/workflows/release.yml` | closure evidence attached in release lane |
| I3 | `adr025-otel-bridge-report` | `.github/workflows/adr025-nightly-otel-bridge.yml` | `otel_bridge_report_v1.json` + `otel_bridge_report_v1.md`, retention 14 days |
| I3 | `adr025-otel-bridge-release-evidence` | `.github/workflows/release.yml` | OTel evidence attached in release lane |

## Reviewer Gates Map

### I1
- `scripts/ci/review-adr025-i1-step1.sh`
- `scripts/ci/review-adr025-i1-step3-c1.sh`
- `scripts/ci/review-adr025-i1-step3-c2.sh`
- `scripts/ci/review-adr025-i1-step3-c3.sh`
- `scripts/ci/review-adr025-i1-step4-a.sh`
- `scripts/ci/review-adr025-i1-step4-b.sh`
- `scripts/ci/review-adr025-i1-step4-c.sh`

### I2
- `scripts/ci/review-adr025-i2-step1.sh`
- `scripts/ci/review-adr025-i2-step2.sh`
- `scripts/ci/review-adr025-i2-step3.sh`
- `scripts/ci/review-adr025-i2-step4-a.sh`
- `scripts/ci/review-adr025-i2-step4-b.sh`
- `scripts/ci/review-adr025-i2-step4-c.sh`
- stabilization:
  - `scripts/ci/review-adr025-i2-stab-a.sh`
  - `scripts/ci/review-adr025-i2-stab-b.sh`
  - `scripts/ci/review-adr025-i2-stab-c.sh`
  - `scripts/ci/review-adr025-i2-stab-d.sh`

### I3
- `scripts/ci/review-adr025-i3-step1.sh`
- `scripts/ci/review-adr025-i3-step2.sh`
- `scripts/ci/review-adr025-i3-step3.sh`
- `scripts/ci/review-adr025-i3-step4-a.sh`
- `scripts/ci/review-adr025-i3-step4-b.sh`
- `scripts/ci/review-adr025-i3-step4-c.sh`
- stabilization:
  - `scripts/ci/review-adr025-i3-stab-a.sh`
  - `scripts/ci/review-adr025-i3-stab-b.sh`
  - `scripts/ci/review-adr025-i3-stab-c.sh`

## Operational Runbooks
- I1 soak enforcement: `docs/ops/ADR-025-SOAK-ENFORCEMENT-RUNBOOK.md`
- I2 closure release integration: `docs/ops/ADR-025-I2-CLOSURE-RELEASE-RUNBOOK.md`
- I3 OTel bridge informational lane: `docs/ops/ADR-025-I3-OTEL-BRIDGE-RUNBOOK.md`
- I3 OTel release integration: `docs/ops/ADR-025-I3-OTEL-RELEASE-RUNBOOK.md`

## Contracts (Current)

### Modes
- release integration modes are `off|attach|warn|enforce`
- default for I2/I3 release integrations is `attach`

### Exit codes
- `0`: pass/attach/off
- `1`: policy fail when explicit policy rules are enabled
- `2`: measurement/contract failure (missing artifact/json, parse/schema mismatch)

### Informational vs release-lane
- nightly soak/readiness/closure/otel workflows are informational lanes
- release workflow integrates fail-closed or non-blocking attach behavior per policy and mode

## Maintenance policy
- Keep updates in small A/B/C slices.
- Use docs-only PRs for index refresh unless a linked contract actually changes.
- If contracts change (artifact names, modes, exit semantics), update source ADR/policy first, then this index.
