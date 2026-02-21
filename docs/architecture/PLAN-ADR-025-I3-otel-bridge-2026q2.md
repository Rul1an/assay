# PLAN — ADR-025 I3 OTel Bridge (2026q2)

## Intent
Iteration 3 introduces an OpenTelemetry bridge as an interoperability layer:
1) **OTel export/import contract** (GenAI semantic conventions + deterministic envelope).
2) **Mapping contract OTel -> Assay evidence** (how OTel data becomes verifiable evidence artifacts).

Step1 is a freeze slice: contracts only (docs + schema + reviewer gate). No runtime.

## Scope (Step1 freeze)
In-scope:
- Documented export/import envelope and determinism constraints
- `otel_bridge_report_v1` schema (strict contract for bridge artifacts)
- Mapping contract section (OTel -> Assay evidence), with explicit non-goals

Out-of-scope (Step1):
- No ingestion/export code changes
- No workflow changes
- No OTel SDK wiring
- No “live” span/event capture
- No dashboard/observability product work

## Part A — OTel export/import contract (frozen)

### Inputs
- OTel traces/spans representing AI-agent execution
- OTel attributes aligned with GenAI semantic conventions (where present)

### Export format (bridge artifact)
- Primary output: `otel_bridge_report_v1.json` (schema in `schemas/otel_bridge_report_v1.schema.json`)
- Optional auxiliary artifacts: raw OTel export payload file(s) stored separately; digest/path references belong in higher-level manifests or system metadata, not in `otel_bridge_report_v1` core fields.

### Envelope requirements (determinism)
- Stable ordering:
  - spans ordered by `(trace_id, span_id)` lexicographically
  - attributes and events sorted deterministically
- Identifier normalization:
  - `trace_id` and `span_id` must be lowercase hexadecimal in persisted bridge reports and match schema patterns (`trace_id`: 32 hex, `span_id`: 16 hex).
  - Exporters that receive mixed/uppercase IDs must normalize to lowercase before emitting `otel_bridge_report_v1.json`.
- Numeric safety:
  - large integers that may exceed JS safe-int must be represented as strings in bridge report (explicit list in schema)
- Redaction:
  - bridge report can include a redaction summary, but must never require secrets to validate
- Time:
  - timestamps preserved if present; additional derived fields must be deterministic

### Compatibility guarantees
- Bridge report is valid without proprietary vendor fields.
- Unknown OTel attributes on spans/events/links are preserved in their corresponding `attributes[]` collections.
- The top-level `extensions` object is reserved for non-attribute vendor/platform metadata; core bridge fields remain strict.

## Part B — Mapping contract: OTel -> Assay evidence (frozen)

### Goal
Define a deterministic mapping from OTel bridge report to Assay evidence primitives:
- Evidence events (CloudEvents-like) representing agent/tool calls
- Manifest extensions to track what was captured/mapped

### Mapping outputs (conceptual)
- Evidence manifest extensions:
  - `x-assay.otel_bridge`:
    - `schema_version`, `classifier_version` (if applicable)
    - `source`: "otel"
    - `trace_count`, `span_count`
    - `mappings_applied[]` (ids/versions/digests)
  - `x-assay.signal_gaps[]` (required vs captured, if applicable)
- Evidence events derived deterministically from spans:
  - tool invocation events
  - model request/response events
  - policy decision events (where present)

### Non-goals (Step1)
- No full OpenTelemetry semantic coverage beyond GenAI + minimal trace context
- No runtime enforcement based on OTel in I3 Step1
- No changes to existing evidence bundle formats

## Exit contracts (future Step2)
Reserved for later (no implementation in Step1):
- 0: bridge pass
- 1: mapping/policy fail
- 2: measurement/contract fail

## Acceptance criteria (Step1)
- Plan + schema define the bridge artifact contract and determinism constraints
- Mapping contract is explicit, testable, and has clear non-goals
- Reviewer gate enforces allowlist-only and workflow-ban
