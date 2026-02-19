# ADR-025 I1 Closure Note

Date: 2026-02-19
Status: Closed on `main`

## What shipped
- Informational nightly soak lane:
  - `.github/workflows/adr025-nightly-soak.yml`
- Informational nightly readiness lane:
  - `.github/workflows/adr025-nightly-readiness.yml`
  - `scripts/ci/adr025-soak-readiness-report.sh`
- Enforcement policy freeze (v1):
  - `docs/architecture/ADR-025-SOAK-ENFORCEMENT-POLICY.md`
  - `schemas/soak_readiness_policy_v1.json`
- Release-lane fail-closed enforcement:
  - `.github/workflows/release.yml`
  - `scripts/ci/adr025-soak-enforce.sh`
  - `scripts/ci/test-adr025-soak-enforce.sh`
- Closure/runbook artifacts:
  - `docs/ops/ADR-025-SOAK-ENFORCEMENT-RUNBOOK.md`
  - `docs/contributing/SPLIT-CHECKLIST-adr025-step4-c-closure.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-adr025-step4-c-closure.md`

## Active knobs (policy v1)
Source of truth: `schemas/soak_readiness_policy_v1.json`

- `classifier_version`: `1`
- Window:
  - `runs_observed_target`: `20`
  - `runs_observed_minimum`: `14`
- Thresholds:
  - `success_rate_min`: `0.90`
  - `contract_fail_rate_max`: `0.05`
  - `infra_fail_rate_max`: `0.01`
  - `unknown_rate_max`: `0.05`
- Exit contract:
  - `pass`: `0`
  - `policy_fail`: `1`
  - `measurement_fail`: `2`

## Enforcement behavior
Release lane enforces readiness fail-closed before publishing steps.

Fail classes:
- `exit 1`: policy threshold violation
- `exit 2`: measurement/contract failure (missing artifact/json, parse errors, classifier mismatch, insufficient window)

## PR-lane safety guarantee
- No ADR-025 fail-closed enforcement in PR lanes.
- No additional required PR checks introduced by ADR-025 I1 Step4.
- Nightly soak/readiness remain informational (`schedule` + `workflow_dispatch`).

## Debug path (fail-closed)
1. Open failed release run and inspect step: `ADR-025 enforce readiness (fail-closed)`.
2. Capture the referenced readiness run id from logs.
3. Verify artifact `adr025-nightly-readiness` exists and contains `nightly_readiness.json`.
4. Re-run locally:
   - `bash scripts/ci/adr025-soak-enforce.sh --policy schemas/soak_readiness_policy_v1.json --readiness <path>`
5. Classify outcome:
   - policy fail (`1`) vs measurement/contract fail (`2`)
6. If needed, re-run nightly readiness (`workflow_dispatch`) and retry release.

## Post-closure checklist (verified)
- `bash scripts/ci/review-adr025-i1-step4-a.sh`
- `bash scripts/ci/review-adr025-i1-step4-b.sh`
- `bash scripts/ci/review-adr025-i1-step4-c.sh`
- `bash scripts/ci/test-adr025-soak-enforce.sh`
