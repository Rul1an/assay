# Interop Matrix Plan

> **Status:** matrix-plan-ready for Slice 5 of the
> agent-observability fidelity roadmap; implemented by the Slice 6
> synthetic harness. This document predeclared the OTel GenAI /
> OpenInference / Runner interoperability coverage matrix before any
> harness or delegated measurement work. It does not translate between
> vocabularies at runtime, does not rank products, and does not publish
> delegated findings.
>
> **Last updated:** 2026-05-28

## Goal

The interop matrix asks one narrow question:

```text
When the same agent run is observed by OTel GenAI vocabulary,
OpenInference vocabulary, and Runner measured effects, which claims can
each layer make about the same tool call, and which claims map, overlap
partially, or fail to express the same evidence boundary?
```

This is not a product comparison and not an automatic converter. It is
a coverage and claim-strength map that reuses the completed calibration,
evidence-pack, semantic-gap, join-result, and claim-class work.

## Prerequisites

| Prerequisite | Status | Why it matters |
|---|---|---|
| Fidelity calibration | Harness-ready | Coverage rows may not treat missing retained trace fields as absence of behavior. |
| Evidence pack carrier | Prototype-ready | Interop examples should be portable and reviewable when Slice 6 adds fixtures. |
| Semantic-gap matrix | Synthetic matrix-ready | The six scenario ids provide the agent shapes and claim-boundary examples. |
| Join contract | Reference-ready: [`join-result-v0.schema.json`](../../reference/observability/schema/join-result-v0.schema.json) exists | Interop rows must state whether a mapping is joined by `tool_call_id`, run-level metadata, trace-local ids, or fallback order. |
| Claim classes | Reference-ready: [`claim-class-cell-v0.schema.json`](../../reference/observability/schema/claim-class-cell-v0.schema.json) exists | Coverage must be expressed as claim support, not "better/worse" vocabulary scoring. |

Slice 5 did not add the `interop_coverage_cell.v0` schema. Slice 6 adds
the sidecar after this plan's row shape survived review.

## Upstream Snapshot

The plan is pinned to the public upstream semantics visible on
2026-05-28. These references are intentionally cited in the plan
because both GenAI semantic conventions and OpenInference conventions
are moving targets.

| Source | Snapshot fact used by this plan |
|---|---|
| [OpenTelemetry GenAI agent and framework spans](https://opentelemetry.io/docs/specs/semconv/gen-ai/gen-ai-agent-spans/) | The page is marked Development and documents the `OTEL_SEMCONV_STABILITY_OPT_IN=gen_ai_latest_experimental` opt-in path for newer GenAI conventions. It also defines agent and tool span operations such as `invoke_agent`, `invoke_workflow`, and `execute_tool`. |
| [OpenTelemetry GenAI events](https://opentelemetry.io/docs/specs/semconv/gen-ai/gen-ai-events/) | GenAI events are also Development, may be language-dependent, and include opt-in content such as structured input/output message details. |
| [OpenInference semantic conventions](https://arize-ai.github.io/openinference/spec/semantic_conventions.html) | `openinference.span.kind` is required for OpenInference spans, with span kinds including `LLM`, `EMBEDDING`, `CHAIN`, `RETRIEVER`, `RERANKER`, `TOOL`, `AGENT`, `GUARDRAIL`, `EVALUATOR`, and `PROMPT`. |
| [openinference-semantic-conventions 0.1.1](https://docs.rs/crate/openinference-semantic-conventions/latest) | The Rust package exposes OpenInference attribute constants and notes dual support for OpenInference-style attributes and OTel GenAI aliases. |

Slice 6 must record the exact package/spec/commit snapshot it uses in every
interop output. If OTel or OpenInference upstream docs move before
implementation, the harness PR should update the snapshot section before
emitting rows.

## Matrix Definition

The matrix has four primary axes. OpenInference span kind is a
vocabulary-specific field, not a fifth Cartesian axis; otherwise the
starter matrix becomes too large before the mappings are even useful.

| Axis | Values | Notes |
|---|---|---|
| Observation profile | `otel_genai_default`, `otel_genai_latest_experimental`, `openinference`, `runner_measured_effects` | The first three values are trace vocabularies. Runner is included as a measured-effects boundary, not as a trace vocabulary. |
| Agent shape | `single_tool_call`, `retry_self_correction`, `runtime_side_effect`, `retrieval_then_tool`, `handoff_multi_agent` | The first three reuse Slice 4 synthetic scenarios. Retrieval and handoff are planned starter extensions for interop because they exercise OpenInference span kinds and OTel workflow/agent spans. |
| Join key | `tool_call_id`, `run_id`, `trace_span_id`, `timestamp_or_order` | Values reuse `assay.observability.join_result.v0`; the matrix must not introduce a new join hierarchy. |
| Evidence layer | `trace_only`, `archive_only`, `joined` | Values describe where the claim is supported. `joined` requires a join-result row; `archive_only` can state measured effects without semantic intent. |

### Vocabulary-Specific Fields

Each row may also carry fields that only apply to one vocabulary:

| Field | Applies to | Examples |
|---|---|---|
| `otel_operation_name` | OTel GenAI | `execute_tool`, `invoke_agent`, `invoke_workflow`, `chat` |
| `otel_semconv_opt_in` | OTel GenAI | `none`, `gen_ai_latest_experimental` |
| `openinference_span_kind` | OpenInference | `TOOL`, `AGENT`, `RETRIEVER`, `GUARDRAIL`, `CHAIN`, `LLM` |
| `runner_effect_kind` | Runner | `filesystem_read`, `filesystem_write`, `runtime_probe`, `network_effect`, `process_execution` |

These fields are intentionally not primary axes. A coverage row can say
"OpenInference expresses this as `TOOL`, OTel expresses this as
`execute_tool`, Runner measures a filesystem read" without pretending
those are the same semantic object.

## Starter Matrix For Slice 6

Slice 6 should implement a starter matrix, not the full Cartesian
product. The first useful harness proves that the row shape works
across all three evidence boundaries and that absence/partial mappings
are first-class outputs.

| Cell id | Scenario / shape | Observation profiles | Join key | Evidence layer | Purpose |
|---|---|---|---|---|---|
| `single_tool_joined_all` | `matched_safe_read` / `single_tool_call` | OTel default, OpenInference, Runner | `tool_call_id` | `joined` | Prove the baseline row can map reported tool intent to measured filesystem read without ranking vocabularies. |
| `hidden_write_joined_all` | `hidden_write` / `single_tool_call` | OTel default, OpenInference, Runner | `tool_call_id` | `joined` | Prove an under-described reported intent can stay a semantic-gap row across vocabularies. |
| `retry_temporal_partial` | `retry_self_correction` | OTel default, OpenInference, Runner | `tool_call_id` | `joined` | Prove terminal-success summaries and full-attempt archives become partial coverage, not false equivalence. |
| `runtime_surface_archive_only` | `runtime_side_effect` | Runner plus trace vocabularies as absent/diagnostic | `run_id` | `archive_only` | Prove runtime effects are measured but not upgraded to tool intent when traces do not express them. |
| `retrieval_then_tool_openinference` | planned synthetic retrieval/tool mix | OpenInference plus OTel latest experimental and Runner | `trace_span_id` or `tool_call_id` when present | `trace_only` and `joined` | Exercise `RETRIEVER` / `TOOL` span-kind coverage without claiming Runner can infer retrieval semantics. |

The first four cells can reuse Slice 4 synthetic fixtures. The fifth is
generated by the Slice 6 interop harness rather than the semantic-gap
harness, but it must remain synthetic and must not publish delegated
measurements.

## Proposed Output Shape

Slice 6 emits `interop_coverage_cell.v0` rows for each starter cell /
observation-profile mapping.
The schema string should be:

```text
assay.experiment.agent_observability_fidelity.interop_coverage_cell.v0
```

Planned fields:

| Field | Type / values | Meaning |
|---|---|---|
| `schema` | const | `assay.experiment.agent_observability_fidelity.interop_coverage_cell.v0` |
| `cell_id` | lowercase id | Stable matrix cell id. |
| `scenario_id` | string | Existing semantic-gap scenario id or planned interop fixture id. |
| `observation_profile` | enum | `otel_genai_default`, `otel_genai_latest_experimental`, `openinference`, `runner_measured_effects`. |
| `source_snapshot` | object | URL, retrieval date, and at least one of package version, semantic-convention tag, or Assay commit. |
| `agent_shape` | enum | One matrix agent shape. |
| `join_key` | enum | Reuse `join_result.v0` join-key vocabulary. |
| `evidence_layer` | enum | `trace_only`, `archive_only`, or `joined`. |
| `coverage_status` | enum | `full`, `partial`, `absent`, or `not_applicable`. |
| `claim_strength` | enum | Reuse `claim_class_cell.v0`: `strong`, `partial`, `weak`, or `absent`. |
| `claim_basis` | enum | Reuse `claim_class_cell.v0`: `reported`, `measured`, `derived`, or `inferred`. |
| `mapping` | object | OTel field, OpenInference field, Runner effect, and Assay claim type when applicable. |
| `mapping_basis` | enum | `explicit_upstream_doc`, `synthetic_fixture`, `derived_join_rule`, or `not_expressible`. |
| `mapping_notes` | string array | Short bounded notes; no freeform product ranking. |
| `non_claims` | string array | Required non-claim identifiers. |

`coverage_status=absent` is a valid result. It means a vocabulary or
layer cannot express the claim in that cell. It is not a test failure
and not a product criticism.

The v0 schema intentionally keeps vocabulary-specific enums tight to the
starter cells. Adding new OTel operation names, OpenInference span
kinds, or Runner effect kinds in a later slice should use a v0.x schema
bump rather than silently widening the meaning of v0.

Example: an OTel GenAI row that tries to express Runner's measured
`filesystem_read` effect should use `coverage_status=absent`,
`mapping_basis=not_expressible`, and a note explaining that no OTel
trace field carries the measured filesystem effect itself. That is a
valid coverage result, not a test failure.

## Mapping Ownership

Slice 6 mappings must be owned by explicit evidence:

- use upstream docs or package constants for vocabulary fields;
- use Slice 4 synthetic fixtures for Runner measured effects and
  semantic-gap scenarios;
- use `join_result.v0` and `claim_class_cell.v0` for join and claim
  strength;
- mark a cell `partial` or `absent` when no explicit upstream field can
  carry the claim.

Do not infer hidden equivalence. If a mapping requires interpretation,
emit `mapping_basis=derived_join_rule` and keep `claim_strength` no
stronger than `partial` unless the source artifacts directly support the
claim.

## Acceptance Rules

- The matrix reports coverage and claim strength, not product ranking.
- Every row must record a source snapshot: upstream URL, retrieval
  date, and at least one version anchor (`package_version`,
  `semconv_tag`, or `assay_commit`).
- Every row must reuse `claim_class_cell.v0` vocabulary for
  `claim_strength` and `claim_basis`.
- Every joined row must reference a `join_result.v0` row or state why no
  join exists.
- Missing cells are valid findings when the vocabulary legitimately does
  not model the behavior.
- `otel_genai_latest_experimental` rows must record the exact
  `OTEL_SEMCONV_STABILITY_OPT_IN` value used by the fixture.
- OpenInference rows that use span kinds must record
  `openinference.span.kind` exactly as emitted.
- Runner rows must remain measured-effect rows. They may not infer tool
  intent without a trace or receipt layer.
- No delegated runs are required or published in Slice 5 or the Slice 6
  starter harness.
- Slice 6 adds the `interop_coverage_cell.v0` schema sidecar only after
  this plan is accepted.

## Non-Claims

- This plan does not rank OTel, OpenInference, Runner, or Assay.
- This plan does not claim semantic equivalence between vocabularies.
- This plan does not publish delegated interop measurements.
- This plan does not promote interop mappings to a product API.
- This plan does not require all three vocabularies to be active in
  production.
- This plan does not define a runtime translator between vocabularies.
- This plan does not claim that an absent field proves absent behavior.

## Exit Gate For Slice 6

Slice 6 is harness-ready when a synthetic interop harness can:

1. Emit `interop_coverage_cell.v0` rows for the five starter cells.
2. Generate at least one all-boundary `tool_call_id` joined example for
   OTel GenAI default, OpenInference, and Runner measured effects.
3. Emit at least one `partial` row and one `absent` row without failing
   the harness.
4. Attach every joined row to a `join_result.v0` row and every coverage
   row to claim-class vocabulary.
5. Carry every starter cell in an evidence pack or a stable synthetic
   output directory without delegated publication claims.

Delegated capture is not part of the Slice 6 exit gate. A later slice
may promote specific rows from synthetic coverage behavior to measured
interop evidence only after it records convention versions, Runner
health, calibration status, and evidence-pack non-claims.
