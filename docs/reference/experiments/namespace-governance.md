# Experiment Namespace Governance

> **Status:** reference guidance for Assay experiment artifacts. This
> document does not define a product API. It keeps experiment-scoped
> schemas, cross-arc fields, and promotion decisions consistent before
> the agent-observability fidelity roadmap adds more artifacts.
> For the broader plan-to-harness-to-summary lifecycle, see
> [`arc-lifecycle-guide.md`](arc-lifecycle-guide.md).

## Problem

Assay experiments now emit several useful but local artifact families:

- overhead samples, summaries, paired sequences, phase timings, and
  event-rate sweep cells;
- cross-runtime drift reports and fixtures;
- observability join and claim-class reference rows;
- active fidelity-calibration sidecars and planned evidence-pack
  artifacts.

Without a naming and promotion rule, each new slice can create a locally
reasonable schema that is hard to compare across arcs later. This doc
sets the default rule before adding more fidelity and evidence-pack
surfaces.

## Naming Convention

Use schema strings in this shape:

```text
assay.experiment.<arc_slug>.<artifact_slug>.v<N>
```

Examples:

```text
assay.experiment.agent_observability_fidelity.calibration.v0
assay.experiment.agent_observability_fidelity.evidence_pack.v0
assay.experiment.runner_vs_otel_overhead.event_rate_sweep.v0
```

Existing pre-governance schemas such as
`assay.experiment.overhead_sample.v0` remain valid. Do not rename
historical artifacts just to fit the new convention. New artifacts
should use the arc/artifact split unless a narrower existing family
already owns the shape.

Rules:

1. **Arc slug first.** The arc names the evidence boundary, not the
   implementation module.
2. **Artifact slug second.** The artifact names what the file contains:
   `sample`, `summary`, `calibration`, `evidence_pack`,
   `paired_sequence`, `phase_timing`, etc.
3. **Version only on shape changes.** Additive optional fields may stay
   within the same version when old artifacts still validate. Changed
   meaning or required fields need a new version.
4. **No product namespace by accident.** `assay.experiment.*` artifacts
   remain local evidence until promoted explicitly.

## Cross-Arc Fields

Prefer repeating a small common field set in each experiment schema over
creating a shared `assay.experiment.common.v0` too early. Duplication is
acceptable while the fields are still proving themselves.

Recommended common fields:

| Field | Meaning |
|---|---|
| `schema` | Schema string for the artifact. |
| `experiment` | Human-readable experiment slug. |
| `assay_commit` | Source commit used to produce the artifact. |
| `started_at` | ISO-8601 timestamp for the sample/run. |
| `host_class` | Host/OS/kernel boundary for measurement claims. |
| `workflow_run_url` | GitHub Actions run URL when produced by delegated workflow. |
| `tool_versions` | Tool/runtime versions relevant to the artifact. |
| `calibration_status` | `clean`, `lossy`, `inconclusive`, or `not_applicable` when the artifact interprets requested-vs-observed signals. |

If three independent arcs need the same nested object with the same
semantics, open a promotion PR to define a shared reference shape under
`assay.observability.*` or another explicit namespace. Do not add a
shared schema as a convenience before it has multiple consumers.

## Promotion Criteria

An experiment artifact may be promoted out of `assay.experiment.*` only
when at least one of these triggers exists:

- A production or CLI feature consumes it directly.
- Two or more experiment arcs independently need the same shape.
- A public reference doc or paper needs the shape as a stable citation
  target.
- External interoperability requires a stable contract.

Promotion targets:

| Target namespace | Use when |
|---|---|
| `assay.runner.*` | The shape is part of Runner archive, projection, or report contracts. |
| `assay.observability.*` | The shape interprets or joins traces, archives, receipts, and external evidence. |
| `assay.receipt.*` or receipt-family docs | The shape becomes a bounded imported evidence receipt. |

Promotion requires:

1. A reference page naming the new stability promise.
2. A migration note for the experiment shape that motivated the
   promotion.
3. At least one validation fixture or golden file.
4. A non-claims section stating what the promoted shape does not prove.

## Fidelity Calibration Shapes

Calibration artifacts should include method metadata. An observed count
without its counting method is not reproducible.

Recommended nested shape:

```json
{
  "schema": "assay.experiment.agent_observability_fidelity.calibration.v0",
  "kind": "sample",
  "calibration_status": "lossy",
  "fidelity_verdict": {
    "runner_capture": "clean",
    "otel_capture": "clipped",
    "overall": "lossy"
  },
  "kernel_events": {
    "target": 1000,
    "observed": 1000,
    "method": "kernel_ndjson_path_match_count",
    "agreement": "match"
  },
  "span_events": {
    "target": 500,
    "observed": 128,
    "method": "otel_trace_json_events_count",
    "agreement": "clipped",
    "effective_limit": 128,
    "effective_limit_source": "default"
  }
}
```

`fidelity_verdict` is the review-facing rollup. The per-measurement
objects are the reproducibility layer. Keep both: a reviewer should see
the verdict quickly, while an auditor can still see how every count was
produced.

### Vocabulary Alignment

The calibration shape uses two vocabulary levels. Per-measurement
`agreement` uses `match`, `clipped`, `drift`, `failed`, or
`not_applicable`. The per-layer `fidelity_verdict` and top-level
`calibration_status` use `clean`, `lossy`, `inconclusive`, or
`not_applicable`. Agreement rolls up to status as follows:
`match -> clean`, `clipped -> lossy`, `drift` or `failed ->
inconclusive`, and `not_applicable -> not_applicable`. Layer statuses
roll up to the overall status by worst case:
`not_applicable < clean < lossy < inconclusive`.

Allowed `agreement` values:

| Value | Meaning |
|---|---|
| `match` | Observed count matches the requested target. |
| `clipped` | Observed count is lower because a known limit applied. |
| `drift` | Observed count differs without a known clipping explanation. |
| `failed` | Counting failed. |
| `not_applicable` | The layer does not apply for this arm or artifact. |

Allowed `method` values should be documented next to the schema that
uses them. Initial methods:

| Method | Meaning |
|---|---|
| `archive_contents_worker_files_count` | Count unique `event-rate-sweep/worker-*` files in extracted archive contents. |
| `kernel_ndjson_path_match_count` | Count matching kernel events in `layers/kernel.ndjson`. |
| `otel_trace_json_events_count` | Count retained OTel span events in trace JSON. |
| `fixture_side_log_count` | Count fixture-emitted records from an explicit side log. |

## Evidence Pack Minimum

The first evidence-pack prototype should keep the mandatory set small:

| Required | Artifact |
|---|---|
| Yes | One-page Markdown summary. |
| Yes | Runner archive or verified archive reference. |
| Yes | Trace JSON or trace reference when a trace layer exists. |
| Yes | Observation health summary. |
| Yes | Redaction manifest, even if it says no redaction was applied. |
| Nice-to-have v1 | Expanded manifest/provenance table. |
| Nice-to-have v1 | Derived measured-effects summary. |

The pack must not strengthen a claim beyond the underlying calibration
and join grades. It is a carrier for evidence, not a new source of
truth.

The v0 prototype lives under
`docs/experiments/agent-observability-fidelity-2026-05/` and uses:

| Schema | Role |
|---|---|
| `assay.experiment.agent_observability_fidelity.evidence_pack.v0` | Pack manifest with scenario, claim class, carried artifacts, health, reproduction, and non-claims. |
| `assay.experiment.agent_observability_fidelity.redaction_manifest.v0` | Required redaction record, even when no redaction was applied. |

Keep this prototype in `assay.experiment.*` until a real CLI or
artifact-exchange consumer needs a stable product surface.

## Semantic-Gap Verdicts

The Slice 4 synthetic harness adds one narrow experiment-scoped verdict
shape:

| Schema | Role |
|---|---|
| `assay.experiment.agent_observability_fidelity.semantic_gap_verdict.v0` | Bounded verdict for the six synthetic scenario-plan rows: `positive_join`, `semantic_gap`, `diagnostic_only`, or `inconclusive`. |

This verdict summarizes existing join and claim-class rows. It does not
replace `assay.observability.join_result.v0`, does not promote semantic
gap findings to a product API, and does not support delegated findings
until the delegated baseline gate is run.

Synthetic fixture payloads emitted by this harness also stay under the
same experiment namespace:

| Schema | Role |
|---|---|
| `assay.experiment.agent_observability_fidelity.synthetic_trace.v0` | Synthetic trace fixture used by the local semantic-gap harness. |
| `assay.experiment.agent_observability_fidelity.synthetic_runner_archive.v0` | Synthetic Runner-archive fixture used by the local semantic-gap harness. |

These fixture payloads are intentionally schema-string-only in v0. They
are not delegated capture artifacts, not Runner archive contracts, and
not a new `assay.synthetic.*` namespace.

## Interop Mapping Rows

The Slice 5 interop plan reserves one experiment-scoped row family for
coverage, joinability, and claim-strength mappings between OTel GenAI,
OpenInference, Runner measured effects, and Assay observability
vocabulary:

| Schema | Role |
|---|---|
| `assay.experiment.agent_observability_fidelity.interop_coverage_cell.v0` | Slice 6 row for observation-profile coverage, row-level joinability, claim strength, join key, evidence layer, source snapshot, and bounded mapping notes. |

The schema sidecar is active in the Slice 6 harness PR. The schema stays
experiment-scoped and must not promote interop mappings to
`assay.observability.*`.

Interop rows must stay coverage-focused:

- use `assay.observability.claim_class_cell.v0` vocabulary for
  `claim_strength` and `claim_basis`;
- use `assay.observability.join_result.v0` vocabulary for join keys;
- keep `joinability` as a row-level summary and not a replacement for
  `assay.observability.join_result.v0`;
- record source snapshots for OTel GenAI and OpenInference because
  both vocabularies are moving;
- treat absent or partial mappings as valid findings, not product
  rankings.

Evidence packs are not required for every matrix cell. Single-run
scenario outputs should use evidence packs when the claim depends on a
portable trace/archive bundle; multi-row synthetic matrix cells may use a
stable directory layout when the claim is coverage-shape behavior rather
than delegated run evidence.

## Artifact Family Inventory

Before adding a new artifact family, check
[`../artifact-families-inventory.md`](../artifact-families-inventory.md).
If the family is still `proposed`, describe it as a working term. Do not
call it a product line, receipt family, or canonical artifact until a
promotion PR says so.

## Non-Claims

- This document does not promote any existing experiment schema.
- This document does not require historical schema renames.
- This document does not define a universal Assay evidence-pack format.
- This document does not make calibration or join rows product APIs.
