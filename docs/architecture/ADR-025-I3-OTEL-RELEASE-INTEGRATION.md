# ADR-025 I3 — OTel Bridge Release Integration (v1)

## Intent
Attach OTel bridge evidence to the **release lane only**, with minimal risk:
- Default mode is **attach** (non-blocking)
- Enforcement (if enabled) is **contract validation first**
- PR lanes remain unchanged (no required-check impact)

## Status Sync (2026-02-25)
- Step4A merged: contract/policy freeze (`schemas/otel_release_policy_v1.json`) + reviewer gate.
- Step4B merged: release wiring + helper script (`scripts/ci/adr025-otel-release.sh`) + deterministic tests.
- Step4C merged: runbook + checklist/review-pack + closure reviewer gate.
- Stabilization merged: Stab A/B/C (`scripts/ci/review-adr025-i3-stab-*.sh`) with determinism edge-case coverage.

## Scope
In-scope (Step4):
- Contract + policy freeze (docs + policy JSON + reviewer gate)
- Release-lane wiring + helper script + deterministic tests
- Runbook/checklist/review-pack closure sync

Out-of-scope (Step4):
- Any enforcement expansion beyond contract validation without a new freeze slice
- OTel SDK wiring / live capture

## Inputs
Nightly OTel bridge artifact (from I3 Step3):
- workflow: `.github/workflows/adr025-nightly-otel-bridge.yml`
- artifact name: `adr025-otel-bridge-report`
- files:
  - `otel_bridge_report_v1.json`
  - `otel_bridge_report_v1.md`
- retention: 14 days

## Mode contract (for Step4B)
- `off`     : skip download/attach
- `attach`  : download + attach evidence; non-blocking (default)
- `warn`    : like attach, but emits explicit warning on missing/invalid artifact; non-blocking
- `enforce` : fail-closed **on contract validation** (exit 2).
  Exit 1 is reserved for future explicit policy rules that are evaluable from the bridge report.

Default: `attach`

## Validation contract (enforce mode baseline)
Contract validation checks (v1):
- JSON parses
- `schema_version == "otel_bridge_report_v1"`
- trace/span ids are lowercase hex and correct length (as per schema)
- unix_nano fields are digit-strings where present
- required top-level fields exist (`source`, `summary`, `traces`)

## Exit contract (for Step4B helper script)
- 0: pass/attach/off
- 1: policy fail (reserved; only when explicit policy rules are introduced)
- 2: measurement/contract fail (missing artifact/json, parse errors, schema mismatch)

## Non-goals
- No PR required-check changes
- No workflow trigger expansion to PR lanes
- No score-based enforcement (bridge report is not a scoring artifact)
