# Assay-Runner Runtime Drift Projection v0.2 Contract

> Internal projection contract slice. This page defines the current
> `assay.runner.runtime_drift.v0.2` report shape used by the
> cross-runtime drift experiment. It also links the retained historical
> v0 schema for older committed reports. It is not a Runner archive
> artifact, not a policy verdict, and not a released product surface.

Runtime drift v0 answers one narrow question:

```text
Given two Runner measured-run archives and declared projection rules,
what observed runtime surface differs, and which differences remain
after those declared projections are applied?
```

The report is descriptive. It reads Runner archive evidence, records raw
set differences by dimension, then adds projection sub-objects for
explicitly declared path and network mappings. It must not rewrite the
source archives, infer semantic workload equivalence, or decide whether
the drift is acceptable.

## Inputs

The report consumes two Runner archives, usually one per runtime arm.
Each archive may be provided as an extracted directory or `.tar.gz`.

| Source | Used For |
|---|---|
| `manifest.json` | Run id and manifest digest anchor |
| `capability-surface.json` | Filesystem paths, network endpoints, process execs, MCP tool surface |
| `observation-health.json` | Health gates copied into report provenance |
| `correlation-report.json` | Correlation status copied into report provenance |
| `layers/sdk.ndjson` | SDK tool events and invocation order |
| `layers/kernel.ndjson` | Open-operation rows when kernel metadata is present |

The comparator may also receive explicit projection configuration:

| Configuration | Effect |
|---|---|
| `--path-alias RAW=PROJECTED` | Projects an observed raw path to a declared logical path |
| `--network-alias ENDPOINT=CLASS` | Projects an exact observed endpoint to a declared network class |
| `--network-cidr CIDR=CLASS` | Projects an IP endpoint by CIDR to a declared network class |

Projection configuration is declared input. The report must not infer it
from path shapes, package names, hostnames, or provider labels.

## Schema

Schema string:

```text
assay.runner.runtime_drift.v0.2
```

Machine-readable schema:

[`schema/runtime-drift-v0.2.schema.json`](schema/runtime-drift-v0.2.schema.json)

Historical v0 reports remain documented by
[`schema/runtime-drift-v0.schema.json`](schema/runtime-drift-v0.schema.json).
v0.2 is a compatible tightening of the projection sub-shape: it keeps
the same report semantics but locks `projection.unmatched_summary` as a
required `{a, b}` object with per-arm `side`, `count`, `samples`, and
`sample_limit` fields.

The literal dotted minor suffix (`v0.2`) is intentional for this
experimental projection contract. Consumers that parse Runner schema
strings should treat the suffix as semver-like (`major.minor`) rather
than matching only `v(\d+)`. Other Runner archive artifacts still use
bare `v0` schema strings until they need a minor compatibility
tightening of their own.

The current comparator emits v0.2 only. Historical v0 reports remain
readable through the retained v0 schema and committed report files, but
new re-renders from this comparator are not intended to reproduce v0
wire output.

Top-level fields:

| Field | Type | Required | Semantics |
|---|---|---:|---|
| `schema` | string | yes | Must equal `assay.runner.runtime_drift.v0.2` for new reports |
| `archive_a` | object | yes | Compact reference to the first input archive |
| `archive_b` | object | yes | Compact reference to the second input archive |
| `taxonomy` | object | yes | Runtime/noise taxonomy vocabulary block |
| `provenance` | object | yes | Report-generation and input-archive provenance |
| `rows` | array | yes | Per-dimension drift rows |
| `summary` | object | yes | Classification counts across rows |

### Archive References

`archive_a.path` and `archive_b.path` are informational location
anchors, not identity fields. For committed re-renders they are normally
repo-root-relative paths so a reviewer can follow them in the repository.
For fresh workflow captures they may be absolute paths on the capture
host. Archive identity comes from `manifest_digest`.

### Provenance Anchors

`provenance.assay_commit` is the **capture** anchor: the Assay revision
that produced the input archives. `provenance.render_metadata` is the
**render** anchor: the comparator revision and render context that
produced the drift report from those archives. These can intentionally
differ when old archives are re-rendered after a projection or schema
change.

Commit anchors are bare Git object identifiers: either full SHAs or
unambiguous 7-64 character lowercase hexadecimal abbreviations. The
upper bound deliberately allows both SHA-1-era 40-character object IDs
and SHA-256-era 64-character object IDs.
`render_metadata.rendered_at` uses RFC3339 date-time syntax such as
`2026-05-25T19:00:35Z`.

This schema uses two SHA conventions:

- Git commit anchors use bare lowercase hex because Git itself resolves
  full or abbreviated object IDs.
- Content-addressed evidence digests use an algorithm prefix such as
  `sha256:<hex>` so future multi-algorithm digest handling stays
  explicit.

Runtime drift v0 uses a required-but-nullable convention for provenance:
schema-critical keys are present even when the caller does not know the
value yet. A present `null` means "unknown/not supplied"; absence means
the report does not satisfy the v0 shape.

## Contract Principles

1. **Raw evidence is preserved.** `only_in_a`, `only_in_b`, and
   `in_both` in each row are raw observed values from the source
   archives.
2. **Projection is additive.** A row may carry a `projection` object,
   but projection output never replaces the raw row values.
3. **Projection rules are named.** Applied projections carry `rules`,
   compact `mappings`, confidence, relation, and taxonomy class
   information.
4. **Unknowns stay visible.** Values without a declared rule remain
   raw or `unknown`; they are not collapsed into a convenient class.
5. **Health is copied, not reinterpreted.** Observation health and
   correlation status appear in `provenance`; runtime drift v0 does not
   recalculate capture health.
6. **Provenance anchors the comparison.** Input manifest digests, Runner
   schema versions, kernel metadata, eBPF object digest, workflow URL,
   capture commit, and render metadata are report metadata when available.
7. **Policy acceptability is out of scope.** A `runtime-induced` row is
   evidence shape, not a policy failure.

## Dimensions

v0 defines these rows:

| Dimension | Source |
|---|---|
| `filesystem_paths_touched` | `capability_surface.filesystem_paths` |
| `kernel_file_operations` | `layers/kernel.ndjson` open metadata, when present |
| `network_endpoints` | `capability_surface.network_endpoints` |
| `process_execs` | `capability_surface.process_execs` |
| `sdk_tool_events` | `layers/sdk.ndjson` tool events |
| `mcp_tool_surface` | `capability_surface.mcp_tools` |
| `tool_invocation_order` | `layers/sdk.ndjson` `tool_call_started` sequence |

The classification labels are:

| Label | Meaning |
|---|---|
| `task-induced` | The dimension overlaps completely between arms |
| `provider-induced` | Non-shared values match a provider-host whitelist |
| `runtime-induced` | Non-shared values are observed and not provider-attributed |
| `inconclusive` | One side lacks data for the dimension |

Classification is intentionally conservative. It describes observed
surface shape; it is not a root-cause proof.

## Projection Sub-Objects

Rows that support projection carry a `projection` object.

| Projection Schema | Applies To |
|---|---|
| `assay.runner.path_projection.v0` | Filesystem paths and operation-aware file values such as `read:/path` |
| `assay.runner.network_projection.v0` | Network endpoints and CIDR-matched IP endpoints |
| `assay.runner.projection_not_applied.v0` | Sentinel for dimensions with no v0 projector |

Each projection object references:

- `assay.runner.runtime_noise_taxonomy.v0` for class vocabulary;
- declared rule names for applied mappings;
- per-value `claim_level` such as `raw_observed`,
  `projected_equivalent`, or `inconclusive`;
- machine-readable `non_claims`.

Rows without an applicable projection carry the
`assay.runner.projection_not_applied.v0` sentinel with
`status=not_applied` so downstream consumers do not confuse absence of
projection with a parser failure.

The v0.2 projection convention keeps `mappings` compact: it lists declared
projected mappings, not every unmatched raw value. Unmatched raw values
are summarized in `unmatched_summary` with per-arm counts and small
samples. The full raw values remain in each row's `only_in_a`,
`only_in_b`, and `in_both` sets.

Operation-aware path values use the shape `op:/absolute/path` before
projection, for example `read:/tmp/run/workdir/input.txt`. The
projection applies only to the absolute path suffix and preserves the
operation prefix. Other colon-separated strings, such as URI-like values
or relative `op:path` values, are treated as one raw value.

## Non-Claims

Runtime drift v0 inherits the projection non-claims from the projection
roadmap:

| Code | Meaning |
|---|---|
| `projection_no_raw_evidence_rewrite` | Raw observed evidence is preserved and remains source of truth |
| `projection_no_semantic_workload_equivalence` | Matching projected values do not prove the workloads are semantically identical |
| `projection_no_policy_acceptability_verdict` | The report does not decide whether drift is acceptable |
| `projection_unknowns_preserved` | Unknown or unclassified values are not collapsed into a class |
| `projection_no_heuristic_noise_taxonomy` | The report does not apply undeclared heuristic taxonomy rules |

Provenance carries its own non-claims for metadata fields, including
that metadata is not a policy verdict and unknowns are preserved.

## Relationship To Other Runner Contracts

- [`artifacts-v0.md`](artifacts-v0.md) remains the primary archive
  evidence contract. Runtime drift v0 reads those artifacts; it does
  not replace them.
- [`cross-runtime-diff-v0.md`](cross-runtime-diff-v0.md) remains the
  narrow Phase 2C capability-surface projection with work-dir prefix
  canonicalization only.
- [`projection-roadmap.md`](projection-roadmap.md) tracks follow-up
  projection layers and kernel event expansion.
