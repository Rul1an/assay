# ADR-025 I2 — Closure Release Runbook

## Purpose
Operational guidance for the ADR-025 I2 closure artifact integration in the **release lane only**.
PR lanes are intentionally unchanged.

## What exists
- Nightly closure artifact workflow (informational): `.github/workflows/adr025-nightly-closure.yml`
  - artifact: `adr025-closure-report`
  - files: `closure_report_v1.json`, `closure_report_v1.md` (retention 14 days)
- Release integration (Step4B):
  - script: `scripts/ci/adr025-closure-release.sh`
  - policy: `schemas/closure_release_policy_v1.json`
  - wired in: `.github/workflows/release.yml` (before publish/attestations)

## Gate modes
Configured via env/input (release workflow):
- `off`     : skip closure download/eval
- `attach`  : download + attach closure artifacts; **non-blocking**
- `warn`    : like attach, but prints WARN on missing/invalid closure; **non-blocking**
- `enforce` : fail-closed on policy/contract violations

Default mode for I2 is **attach**.

## Exit contract (release integration)
The release step's decision is determined by `scripts/ci/adr025-closure-release.sh`:
- `0` pass/attach (or off)
- `1` policy/closure fail (only blocks in `enforce`)
- `2` measurement/contract fail (missing artifact/json, parse errors, schema mismatch)

## Stabilization hardening notes (Stab B)
- Script now emits startup diagnostics:
  - `mode`, `policy`, `out_dir`, `workflow`, `test_mode`
- `violations` type contract:
  - missing or `null` => treated as empty list
  - non-list value (for example string/object) => measurement/contract failure
- Test-only knobs (must not be used in production release wiring):
  - `ASSAY_CLOSURE_RELEASE_TEST_MODE=1`
  - `ASSAY_CLOSURE_RELEASE_LOCAL_JSON=/path/to/closure_report_v1.json`
  - `ASSAY_CLOSURE_RELEASE_SIMULATE_MISSING_ARTIFACT=1`

## Triage snippets (log-first)
Expected startup line:
- `ADR-025 closure: mode=<mode> policy=<path> out_dir=<path> workflow=adr025-nightly-closure.yml test_mode=<0|1>`

Common signals:
- Missing token:
  - `Measurement error: missing GH_TOKEN`
- No successful nightly closure run:
  - `Measurement error: could not find successful adr025-nightly-closure.yml run`
- Download failure with debug tail:
  - `missing closure_report_v1.json in downloaded artifact; gh run download output: ...`
- Type mismatch in report:
  - `Measurement error: closure report violations must be a list if present`

## Common incidents & response

### A) Missing closure artifact / missing closure_report_v1.json
Symptoms:
- Script prints "missing closure_report_v1.json in artifact"
- Exit:
  - `attach`: continues (non-blocking), artifacts may be absent
  - `warn`: continues with WARN
  - `enforce`: exit `2` (blocks release)

Actions:
1) Check nightly closure workflow health and latest run outcome.
2) Confirm artifact name is `adr025-closure-report`.
3) Re-run nightly closure via workflow_dispatch if needed.
4) If log contains `gh run download output:`, use that tail to triage auth/artifact name issues first.

### B) Classifier mismatch / schema mismatch / invalid JSON
Symptoms:
- Script prints "unexpected closure schema_version" or parse errors.
- Exit:
  - `attach`/`warn`: non-blocking, logs decision
  - `enforce`: exit `2` (blocks release)

Actions:
1) Verify `closure_report_v1.json` was produced by current `adr025-closure-evaluate.sh`.
2) Verify policy file version aligns (`schemas/closure_release_policy_v1.json`).
3) If mismatch is due to a planned contract change: update policy/contracts first (new freeze slice).

### D) Violations field wrong type
Symptoms:
- Script prints `closure report violations must be a list if present`
- Exit:
  - `attach`/`warn`: non-blocking continuation with measurement warning
  - `enforce`: exit `2` (blocks release)

Actions:
1) Inspect generating source for `violations`; ensure array or null/missing.
2) Re-run closure evaluator fixture tests locally.
3) Only adjust schema/contract via freeze slice PR if the type contract must change.

### C) Score below threshold
Symptoms:
- In enforce mode: "closure score < threshold"
- Exit:
  - `enforce`: exit `1` (blocks release)
  - `attach`/`warn`: logs score and proceeds

Actions:
1) Inspect `closure_report_v1.md` for gaps/violations.
2) Address completeness/provenance gaps (inputs/manifest extensions).
3) If threshold is too strict: change policy via a freeze PR (do not hotfix in release).

## Break-glass / override
Use break-glass only for time-critical releases with explicit release ownership.
Rules:
- Override must be explicit (set mode to `off` or `attach`) and auditable (workflow run inputs/logs).
- Do not silently bypass enforcement in code.
- After incident, revert to default **attach** and file a follow-up issue describing:
  - root cause
  - whether policy/inputs need adjustment
  - whether to upgrade to `warn`/`enforce` later

## Verification commands (local)
- Reviewer gates:
  - `bash scripts/ci/review-adr025-i2-step4-a.sh`
  - `bash scripts/ci/review-adr025-i2-step4-b.sh`
  - `bash scripts/ci/review-adr025-i2-step4-c.sh`
