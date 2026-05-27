# Assay Artifact Families Inventory

> **Status:** orientation inventory. This page classifies current and
> proposed artifact families as canonical, reference, experiment-scoped,
> or proposed. It does not promote any artifact and does not define a new
> schema.

## Why This Exists

Assay now has several evidence-bearing artifact families: Trust Card,
Trust Basis, Runner archives, receipt families, observability join rows,
experiment sidecars, and planned fidelity/evidence-pack outputs. This
inventory keeps those families legible so new experiments do not
accidentally present proposed artifacts as canonical product surfaces.

## Status Classes

| Status | Meaning |
|---|---|
| `canonical` | Product or release-line artifact family with existing user-facing meaning. |
| `reference` | Stable research/reference vocabulary or schema family used to interpret evidence. |
| `experiment-scoped` | Local measurement or comparison artifact for one experiment line. |
| `proposed` | Planned or working-term artifact family; not yet a stable contract. |
| `historical` | Kept for traceability, not a recommended new surface. |

## Current Families

| Family | Status | Namespace / docs | Role |
|---|---|---|---|
| Trust Card | `canonical` | CLI/reference docs | User-facing claim summary surface. |
| Trust Basis | `canonical` | CLI/reference docs | Lower-level evidence basis for trust claims. |
| Runner archive | `canonical` | `assay.runner.*` | Measured-run evidence captured by Runner. |
| Runner projection/report schemas | `reference` | `docs/reference/runner/` | Runner-adjacent reports, diffs, and projections. |
| Receipt families | `reference` | [`receipt-families.md`](receipt-families.md) | Bounded imported evidence receipts. |
| Observability claim classes | `reference` | [`observability/claim-classes-v0.md`](observability/claim-classes-v0.md) | Vocabulary for what traces, archives, and joined artifacts can honestly claim. |
| Observability join rows | `reference` | [`observability/join-contract-v0.md`](observability/join-contract-v0.md) | Join-grade rows for trace/archive/receipt comparisons. |
| Overhead experiment sidecars | `experiment-scoped` | `assay.experiment.*` under `runner-vs-otel-overhead-2026-05/` | Samples, summaries, phase timings, paired sequences, and event-rate sweep cells. |
| Cross-runtime drift outputs | `experiment-scoped` | `cross-runtime-drift-2026-05/` | Runtime capability-surface drift comparisons. |
| Fidelity calibration | `proposed` | `assay.experiment.agent_observability_fidelity.calibration.v0` | Requested-vs-observed fidelity verdicts and per-layer count methods. |
| Evidence pack | `proposed` | `assay.experiment.agent_observability_fidelity.evidence_pack.v0` | Portable bundle carrier for one run or scenario. |
| Binding evidence / join receipts | `proposed` | undecided | Working term for tool-call input/output/effect binding evidence. Not a product line yet. |
| Semantic-gap finding | `proposed` | undecided | Experiment result family for reported-intent vs measured-effect divergence. |
| Interop mapping | `proposed` | undecided | Mapping rows between OTel GenAI, OpenInference, Runner, and Assay claim vocabulary. |

## Promotion Rule

Proposed or experiment-scoped artifacts should not be described as
canonical until a promotion PR names:

1. the consumer that needs the artifact;
2. the namespace and stability promise;
3. the validation fixtures or golden examples;
4. the migration path from the experiment artifact, if any;
5. the non-claims the promoted artifact still carries.

See
[`experiments/namespace-governance.md`](experiments/namespace-governance.md)
for naming and promotion details.

## Non-Claims

- This inventory does not create new artifact families by itself.
- This inventory does not require current experiment artifacts to be
  renamed.
- `proposed` means "useful working term," not "committed product
  surface."
