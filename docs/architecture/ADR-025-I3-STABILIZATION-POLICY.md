# ADR-025 I3 Stabilization Policy (v1)

## Intent
Harden the I3 OTel bridge generator with minimal blast radius:
- Add deterministic edge-case fixtures and tests
- Clarify determinism invariants and failure modes
- Keep workflows unchanged during stabilization slices

## Scope (stabilization)
In-scope:
- `scripts/ci/adr025-otel-bridge.sh` (parsing/sorting/normalization hardening)
- Additive fixtures under `scripts/ci/fixtures/adr025-i3/`
- Deterministic tests in `scripts/ci/test-adr025-otel-bridge.sh`
- Docs/checklist sync + stabilization reviewer gates

Out-of-scope:
- Any `.github/workflows/*` edits (stabilization does not touch Step3 lane)
- Any Rust runtime changes
- Any PR required-check changes
- OTel SDK wiring / live capture

## Contracts to preserve (must not change)
### Exit contract (script)
- 0: report generated successfully
- 2: measurement/contract failure (invalid JSON, missing required fields, invalid IDs/timestamps)

### Core contract (I3 Step1)
- `schema_version == "otel_bridge_report_v1"`
- `trace_id` and `span_id` normalized to lowercase hex (32/16)
- Unknown OTel attrs remain in `attributes[]` (KV list)
- `extensions` is only for non-attribute metadata (e.g., resource)

### Determinism invariants (freeze)
- Sorting:
  - traces sorted by `trace_id`
  - spans sorted by `span_id` (within trace)
  - attributes sorted by `key`
  - events sorted by `(time_unix_nano, name)`
  - links sorted by `(trace_id, span_id)`
- Time encoding:
  - `*_time_unix_nano` encoded as digit-strings in output

## Hardening goals (Stab B)
Add fixtures/tests for edge cases:
- Multiple traces/spans ordering
- Attributes ordering stability
- Events ordering stability
- Links ordering stability
- Mixed input types for unix_nano (int vs digit-string) => output always digit-string
- Uppercase IDs normalization
- Contract failures remain exit 2

## Docs sync goals (Stab C)
- Update I3 Step3 checklist/review-pack to mention deterministic edge-cases covered
- Optional: add short ops note about interpreting bridge report ordering
