# ADR-025 I3 — OTel Release Integration Runbook

## Purpose
Operational guidance for ADR-025 I3 OTel bridge integration in the **release lane only**.
PR lanes remain unchanged.

## What exists
- Nightly OTel bridge workflow (informational): `.github/workflows/adr025-nightly-otel-bridge.yml`
  - artifact: `adr025-otel-bridge-report`
  - files: `otel_bridge_report_v1.json`, `otel_bridge_report_v1.md` (retention 14 days)
- Release integration (Step4B):
  - script: `scripts/ci/adr025-otel-release.sh`
  - policy: `schemas/otel_release_policy_v1.json`
  - wired in: `.github/workflows/release.yml` (before provenance/release publish)

## Gate modes
Configured via release workflow variable:
- `off`     : skip OTel download/evaluation
- `attach`  : download + attach OTel evidence; **non-blocking**
- `warn`    : like attach, but emits explicit WARN on measurement issues; **non-blocking**
- `enforce` : fail-closed on **contract validation** failures

Default mode for I3 Step4 is **attach**.

## Enforce semantics (contract-only)
Policy v1 freezes enforce behavior as contract-only:
- `enforce_semantics.mode = "contract_only"`
- `policy_rules_enabled = false`

Implication:
- exit `1` (policy fail) is reserved for future explicit rules.
- current enforce behavior primarily returns exit `2` on contract/measurement failures.

## Contract validation baseline
The helper validates:
- JSON parse success
- `schema_version == "otel_bridge_report_v1"`
- required top-level contract (`source.kind`, `summary`, `traces`)
- lowercase hex IDs (`trace_id`, `span_id`, optional `parent_span_id`, link IDs)
- unix-nano fields are digit-strings where required
- attribute/event/link container shapes

## Exit contract
From `scripts/ci/adr025-otel-release.sh`:
- `0`: pass/attach/off (or non-blocking continuation in attach/warn)
- `1`: policy fail (reserved until explicit policy rules exist)
- `2`: measurement/contract fail (missing artifact/json, parse errors, schema mismatch)

## Decision audit log
The script emits one machine-parseable JSON decision line per run:
- `event = "adr025.otel_release_decision"`
- includes `mode`, `decision`, `exit_code`, plus `policy_path`, `report_path`, `run_id`
- `score`/`threshold` are currently `null` for OTel release decisions

## Common incidents & response

### A) Missing artifact / missing `otel_bridge_report_v1.json`
Symptoms:
- log includes `missing otel_bridge_report_v1.json in downloaded artifact`
- behavior:
  - `attach`: continue, non-blocking
  - `warn`: continue with warning
  - `enforce`: exit `2` (blocks release)

Actions:
1) Check latest run of `adr025-nightly-otel-bridge.yml`.
2) Confirm artifact name is exactly `adr025-otel-bridge-report`.
3) Re-run nightly workflow via `workflow_dispatch` if needed.

### B) Schema/contract mismatch
Symptoms:
- `unexpected otel schema_version` / invalid ID or unix-nano type logs
- behavior:
  - `attach`/`warn`: non-blocking
  - `enforce`: exit `2`

Actions:
1) Verify report producer: `scripts/ci/adr025-otel-bridge.sh`.
2) Verify policy file: `schemas/otel_release_policy_v1.json`.
3) If contract change is intentional, land a new freeze slice first.

### C) Unexpected policy-fail path
Symptoms:
- enforce returns exit `1`

Interpretation:
- with policy v1 (`policy_rules_enabled=false`), this should be rare and signals policy semantics drift.

Actions:
1) Inspect current policy JSON in repo.
2) Confirm release workflow references policy v1 path.
3) Revert unintended policy semantic change via freeze PR.

## Break-glass / override
Use only for time-critical release-owner decisions:
- set `ASSAY_OTEL_GATE` to `off` or `attach` explicitly in release context
- keep override auditable through workflow logs/inputs
- never introduce silent bypass in script/workflow code

After incident:
1) restore default `attach`
2) file follow-up with root cause and contract decision

## Verification commands (local)
- `bash scripts/ci/test-adr025-otel-release.sh`
- `bash scripts/ci/review-adr025-i3-step4-a.sh`
- `bash scripts/ci/review-adr025-i3-step4-b.sh`
- `bash scripts/ci/review-adr025-i3-step4-c.sh`
