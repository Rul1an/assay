# Schema Namespace Overview

> **Status:** orientation index. This page does not define a schema. It
> records the current top-level Assay schema namespaces, their scope, and
> their stability promises.

Assay uses schema strings to state what kind of artifact is being
validated. A file path can help humans find the sidecar, but the schema
string carries the contract boundary.

## Namespace Rules

| Namespace | Scope | Stability Promise | Sidecar Convention |
|---|---|---|---|
| `assay.runner.*` | Runner archive, evidence, projection, and report contracts consumed by Runner-adjacent tooling. | Reference contracts unless marked historical or embedded vocabulary. | Reference docs under [`runner/`](runner/), with JSON sidecars under [`runner/schema/`](runner/schema/) when machine validation is active. |
| `assay.experiment.*` | Time-limited measurement evidence for a named experiment line. | Experiment-scoped; may change between slices and does not become product surface without promotion. | Sidecars live under the owning experiment's `schema/` directory. |
| `assay.observability.*` | Research/reference vocabulary for comparing traces, measured-run archives, joined artifacts, and external receipts. | Reference vocabulary for research output; not a Runner archive artifact and not experiment-scoped measurement evidence. | Reference docs under [`observability/`](observability/), with JSON sidecars under [`observability/schema/`](observability/schema/). |

## Governance

- A new top-level namespace should add a row here before it becomes
  a reviewable surface.
- Moving an artifact from `assay.experiment.*` into
  `assay.runner.*` or another reference namespace requires a reference
  page or ADR that states the new stability promise.
- `assay.observability.*` contracts can cite Runner and experiment
  artifacts, but they do not promote those artifacts into new evidence
  claims by themselves.
- Runner archive health gates remain Runner-owned even when an
  observability comparison uses their results.
- Compliance, legal, and product-positioning claims require their own
  docs. Schema validation alone does not create those claims.

## Detailed Indexes

| Index | Role |
|---|---|
| [`runner/schemas-overview.md`](runner/schemas-overview.md) | Detailed Runner and Runner-adjacent schema list. |
| [`artifact-families-inventory.md`](artifact-families-inventory.md) | Canonical, reference, experiment-scoped, and proposed artifact-family inventory. |
| [`observability/README.md`](observability/README.md) | Observability reference contracts for claim classes and joins. |
| [`experiments/namespace-governance.md`](experiments/namespace-governance.md) | Naming, promotion, and cross-arc field guidance for experiment-scoped schemas. |
