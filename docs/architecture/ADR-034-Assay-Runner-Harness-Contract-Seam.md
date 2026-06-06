# ADR-034: Assay / Runner / Harness Contract Seam

## Status
Proposed (June 2026)

This is a skeleton ADR. It fixes the decision and the boundary now; the concrete
schema enumeration and the compatibility-gate mechanics are detailed during
implementation of the slices that depend on it.

## Context

The Assay ecosystem is three components that release on different cadences:

- **Assay (core):** produces canonical artifacts (evidence bundles, coverage
  reports, claim-class verdicts).
- **Assay-Runner** (`crates/assay-runner-schema|core|linux`): produces the
  observation/projection schema (path, network, and kernel-event projections with
  claim levels; see `docs/reference/runner/projection-roadmap.md`). The crates
  publish to crates.io and are standalone-useful. Repository extraction is gated by
  `docs/reference/runner/extraction-roadmap.md`.
- **Assay-Harness** (separate repository): a recipe, gate, and report consumer over
  canonical Assay and Runner artifacts. It already documents how it consumes the
  runner schema and how it tracks compatibility.

Today these three are coupled only by data shapes, but those shapes are described in
several places at once: the runner projection roadmap, the Harness consumption and
compatibility docs, and the claim-class cell description. There is no single
governed contract, so a change in one producer can silently break a consumer, and
the absence of an explicit seam makes it tempting to couple a consumer to producer
internals.

## Decision

Treat the seam between producers and consumers as a single, explicitly versioned
contract whose source of truth is the published data shapes:

1. the runner projection schema,
2. the claim-class cell vocabulary, and
3. the canonical artifact manifest shape.

Rules:

- **Producers** (Assay core, Assay-Runner) and **consumers** (Assay-Harness, and
  any future consumer such as an eval scorer or an OTel exporter) depend only on the
  published shapes, never on each other's internals.
- The contract carries an explicit version. Changes to a published shape go through
  a compatibility review before release, following the compatibility discipline the
  Harness already uses for trust-basis families.
- The Runner stays standalone-useful: no consumer may depend on Runner internals, so
  the options in the runner extraction roadmap remain open (see ADR for the runner
  standalone boundary, lifted alongside this one).

## Contract surface (skeleton)

The contract is the union of the three shapes above. This ADR does not re-specify
them; it names them as the governed surface and points at their current homes:

| Shape | Producer | Current source of truth |
|-------|----------|-------------------------|
| Runner projection schema | Assay-Runner | `docs/reference/runner/projection-roadmap.md` |
| Claim-class cell vocabulary | Assay core | claim-class documentation / examples |
| Canonical artifact manifest | Assay core | evidence bundle manifest (schema v1) |

Implementation of the first dependent slice promotes these into a single versioned
contract reference and adds the compatibility gate.

## Consequences

- The three components can release independently without silent breakage.
- The Runner remains extraction-ready because nothing depends on its internals.
- New consumers (OTel exporter, eval scorer, attestation predicate) attach to the
  contract rather than to a specific producer build.
- Cost: a contract-version surface and a compatibility-review step to maintain.

## Non-claims

- This ADR governs the wire shapes only. It does not freeze any component's
  internals and does not prescribe a repository structure.
- It makes no product, packaging, or repository-split decision; the runner
  standalone boundary and its extraction roadmap own that question.

## References

- `docs/reference/runner/projection-roadmap.md`
- `docs/reference/runner/extraction-roadmap.md`
- Assay-Harness consumption and compatibility docs (separate repository)
