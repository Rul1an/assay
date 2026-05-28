# Claim-Boundary Positioning

> **Status:** reference positioning note. Last updated: 2026-05-28.
> This document does not open a new experiment arc, define a schema, or
> promote any `assay.experiment.*` artifact to a product API.

## Position

Assay is not an observability replacement, trace viewer, vendor ranking,
or general agent dashboard. Assay's strongest post-arc position is a
claim-boundary and evidence-fidelity layer for agent systems.

The closed Runner-vs-OTel overhead arc and agent-observability fidelity
arc support one shared statement:

> Assay helps determine what an agent run proves, not just what it
> emitted.

The useful boundary is not "Runner versus OTel" or "OpenInference versus
OTel GenAI." It is the boundary between reported intent, measured
effect, calibration health, join strength, and the claim a reviewer may
safely make.

## Methodology Anchor

Future agent-observability work should start from the pattern proven by
the two closed arcs:

1. **Calibrate before timing.** Requested signals must be compared with
   observed signals before throughput, timing, or absence claims are
   interpreted.
2. **Respect evidence boundaries.** In-process traces, OpenInference or
   OTel vocabularies, Runner archives, policy evidence, and kernel
   effects carry different claim surfaces.
3. **Classify claims.** A trace/archive mismatch is a measured
   divergence, not automatically malicious behavior, policy failure, or
   root cause.
4. **Carry bounded evidence.** A portable carrier should make review
   easier without strengthening the underlying evidence.
5. **Use delegated gates sparingly.** Real infrastructure should verify
   a specific publication gate, not broaden a synthetic experiment by
   accident.

This is the product-facing form of the experiment lifecycle documented
in [`../experiments/arc-lifecycle-guide.md`](../experiments/arc-lifecycle-guide.md).

## Public Boundary

This document records public positioning and selection discipline. It
does not publish adjacent-whitespace shortlists, outreach targets,
comment drafts, competitor analysis, or private sequencing notes.

The public rule is simple: arc closure is a stop condition, not
permission to open every adjacent question. A new arc requires a named
consumer, upstream response, stable contract ask, or concrete delegated
publication gate.

## Selection Rule

Open a new arc only when the question can be stated as:

> Which claim can we not safely make today because trace, protocol,
> policy, identity, or runtime evidence is not yet bound tightly enough?

Prefer future arcs that:

- create or validate a bounded claim class;
- bind two or more evidence layers that currently drift apart;
- make absence, partial coverage, clipping, or weak joins first-class;
- produce a carrier a reviewer can inspect;
- have a delegated baseline or a concrete downstream consumer.

Defer arcs that:

- mainly rank tools, vendors, vocabularies, or frameworks;
- add another dashboard without changing claim strength;
- expand a synthetic matrix without a new claim boundary;
- require schema promotion before a consumer exists;
- duplicate OTel/OpenInference trace viewing instead of binding traces
  to measured effects.

## Source Anchors

- Assay overhead arc:
  [`../../experiments/runner-vs-otel-overhead-2026-05/findings-summary.md`](../../experiments/runner-vs-otel-overhead-2026-05/findings-summary.md)
- Assay fidelity arc:
  [`../../experiments/agent-observability-fidelity-2026-05/findings-summary.md`](../../experiments/agent-observability-fidelity-2026-05/findings-summary.md)
- Assay arc lifecycle:
  [`../experiments/arc-lifecycle-guide.md`](../experiments/arc-lifecycle-guide.md)

## Non-Claims

- This document does not open or prioritize adjacent experiment arcs.
- This document does not promote evidence packs, calibration verdicts,
  interop rows, semantic-gap verdicts, or future binding evidence to
  product APIs.
- This document does not publish outreach plans, target lists, comment
  drafts, competitive analysis, or private sequencing notes.
- This document does not rank trace vocabularies, protocols, Runner, or
  Assay as products.
- This document does not claim that every protocol, trace, or
  trace/kernel mismatch is a security issue.
