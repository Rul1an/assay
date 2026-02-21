# ADR-025 I3 OTel Bridge Runbook

## Purpose
Operational guide for the ADR-025 I3 OTel bridge informational lane.
This lane produces deterministic bridge artifacts for audit/debug and does not gate PR lanes.

## What runs
- Workflow: `.github/workflows/adr025-nightly-otel-bridge.yml`
- Generator: `scripts/ci/adr025-otel-bridge.sh`
- Input fixture (Step3): `scripts/ci/fixtures/adr025-i3/otel_input_minimal.json`
- Uploaded artifact: `adr025-otel-bridge-report`
  - `otel_bridge_report_v1.json`
  - `otel_bridge_report_v1.md`

## Determinism invariants
- `trace_id` sorted lexicographically across traces.
- `span_id` sorted lexicographically within each trace.
- `attributes[]` sorted by key.
- `events[]` sorted by `(time_unix_nano, name)`.
- `links[]` sorted by `(trace_id, span_id)`.
- `trace_id`/`span_id` persisted as lowercase hex.
- `*_time_unix_nano` persisted as digit-strings.

## Exit contract (generator)
- `0`: bridge report generated.
- `2`: measurement/contract failure (input shape, missing required fields, invalid IDs/timestamps).

## Quick triage
### Missing artifact output
- Verify workflow run completed and upload step ran (`if: always()`).
- Confirm artifact name is exactly `adr025-otel-bridge-report`.

### Contract/shape failure
Typical generator messages:
- `Measurement error: top-level traces must be array`
- `Measurement error: span_id must be string`
- `Measurement error: expected unix_nano int or digit-string`

Actions:
1. Re-run local tests for deterministic fixtures.
2. Validate input fixture shape.
3. If contract change is intentional, update Step1 freeze docs/schema first.

## Local verification
- `bash scripts/ci/test-adr025-otel-bridge.sh`
- `BASE_REF=origin/main bash scripts/ci/review-adr025-i3-step3.sh`
- `BASE_REF=origin/main bash scripts/ci/review-adr025-i3-stab-b.sh`

## Notes
- Unknown OTel attrs must remain under `attributes[]` entries in the report.
- Top-level `extensions` is reserved for non-attribute metadata.
