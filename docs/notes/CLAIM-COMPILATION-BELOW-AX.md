# Claim Compilation Below Agent Experience

> **Status:** positioning note
> **Last updated:** 2026-05-06
> **Scope:** explains how Assay relates to the emerging Agent Experience (AX)
> layer without claiming to own AX, add a new receipt family, or add new Trust
> Basis semantics.

Assay compiles agent-system outcomes into bounded, scoped claims.

Those claims are useful for AX-ready systems, ad-hoc agent test runs, and CI
review alike. The positioning is structural: if "Agent Experience" becomes a
durable category, Assay is the evidence layer beneath it. If the term fades,
Assay is still the claim-compilation layer for agent outcomes.

## Context: AX Is A Layer Above Assay

A category called Agent Experience is forming around making products easier for
agents to consume:

- Netlify frames Agent Experience as the holistic experience AI agents have as
  users of a product or platform.
- Stainless focuses on agent-ready APIs, generated MCP servers, typed SDK use,
  and documentation search for coding agents.
- Nx describes agentic experience as increasingly important alongside
  developer experience, with CLIs and workflows shaped for AI agents.
- OpenAI and Anthropic describe harness engineering and harness design as
  practical work around agent loops, scaffolding, long-running execution, and
  agent-legible environments.

The shared move is making systems easier for agents to consume.

Assay does not compete in that layer. Assay can operate around MCP and ships
policy/evidence surfaces for agent tool use, but it is not a docs platform, SDK
generator, MCP-generation service, hosted AX control plane, or eval platform.

This note explains the layer Assay does occupy.

## What Assay Does

Assay performs claim compilation: it takes a bounded outcome or runtime signal
and produces a portable evidence artifact plus a scoped claim about that
artifact.

Every supported receipt path answers four questions:

1. **What was observed?** The bounded evidence body.
2. **Where did it come from?** Source artifact, digest, and reducer provenance.
3. **What claim can be compiled?** A stable Trust Basis claim id and boundary.
4. **What is not claimed?** Explicit non-claims such as correctness, safety, or
   completeness.

Trust Basis is Assay's machine-readable scoped claim artifact. It is not a
trust score. It is a claim table keyed by stable `claim.id` values, with levels
and boundaries derived from verified evidence.

The fourth question is the differentiator. Most evidence systems leave the
claim boundary to the downstream reader. Assay tries to make the boundary part
of the artifact: observed here, reduced this way, claimable only under this
scope, and not evidence of broader truth.

## Why Claim Compilation Is The Wedge

Adjacent tools produce useful outputs, but not scoped Assay-style claims:

| Source | Their output | Assay compilation |
|---|---|---|
| Promptfoo | selected assertion result | bounded eval-outcome receipt and `external_eval_receipt_boundary_visible`; no model-correctness claim |
| OpenFeature | selected boolean `EvaluationDetails` outcome | bounded runtime-decision receipt and `external_decision_receipt_boundary_visible`; no flag-config correctness claim |
| CycloneDX | selected ML-BOM model component | bounded inventory/provenance receipt and `external_inventory_receipt_boundary_visible`; no BOM-completeness claim |
| Runtime trace events | observed capability events such as filesystem, network, process, or tool decisions | bounded runtime evidence and reviewable capability surface; no claim that policy is correct |

In this model, the adjacent system produces the outcome. Assay compiles the
bounded claim around that outcome.

The first three rows above are not hypothetical. They are the currently released
claim-visible receipt families, with checked-in proof artifacts in
[Evidence Receipts in Action](EVIDENCE-RECEIPTS-IN-ACTION.md) and source-of-truth
metadata in the [receipt family matrix](../reference/receipt-family-matrix.json).

## Where Assay Sits

AX is about what agents experience when consuming systems: agent-facing
contracts, SDKs, docs, MCP servers, CLIs, and harness ergonomics.

Assay is about whether the outcomes those systems produce are inspectable,
portable, and reviewable.

```text
AX layer
  agent-facing contracts, SDKs, docs, MCP servers, harness ergonomics
  examples: Netlify, Stainless, Nx, OpenAI/Anthropic harness work
        |
        v
Assay layer
  bounded receipts + Trust Basis scoped claims over agent outcomes
        |
        v
Evidence sources
  Promptfoo, OpenFeature, CycloneDX, runtime traces, CI test runs
```

Assay does not require AX. Many agent systems in 2026 are not AX-ready: they are
plain tests around HTTP calls, local tools, function-calling interfaces, shell
commands, or ad-hoc prompt loops. Assay can still review the capability evidence
those runs produce.

AX-ready systems are one sharp wedge. The broader surface is every agent test
run that produces capability evidence.

## Assay Harness

Assay Harness sits next to Assay. It orchestrates recipes, gates, and reviewer
projections over canonical Assay artifacts.

Harness can:

- compose recipes that combine receipts from multiple sources;
- run Trust Basis assertions and diffs across receipt sets;
- project artifacts into CI summaries, JUnit, Markdown, and other review
  surfaces.

Harness does not:

- run evals for Promptfoo;
- serve feature flag decisions for OpenFeature;
- produce CycloneDX inventories;
- provide AX itself.

Harness coordinates claim artifacts. It does not take over the work of the
upstream systems that produce source outcomes.

## Non-Claims

To support AX-adjacent systems without competing with them, Assay does not
claim to:

- own Agent Experience as a category;
- build a docs platform for agents;
- generate agent-facing SDKs;
- run evals;
- replace runtime guardrails or platform egress controls;
- certify correctness, safety, or legal compliance.

Stretching Assay into those claims would replace a clean adjacency with a
contested overlap.

## Operating Principles

**Machine-readable first.** Receipt schemas, receipt-family matrix entries,
canonical diff JSON, and Trust Basis claim ids must be parseable by other tools
without human translation.

**Copyable proof.** Released examples should link to small artifacts, versioned
recipes, verified bundles, and canonical projections rather than floating
narrative claims.

**Bounded guarantees.** Each receipt should say what it proves, what it does not
prove, and where the boundary sits. Bounded evidence is more useful in review
than broad assurance language that collapses under due diligence.

## What This Note Does Not Commit To

- No prediction that AX will become the lasting industry term.
- No prediction about which AX-layer tools will dominate.
- No claim that any Assay artifact satisfies a specific compliance
  certification.
- No new receipt family, Trust Basis claim, Harness recipe, or public
  integration claim.

## In One Sentence

Assay compiles agent-system outcomes into bounded, scoped claims, usable as
evidence for AX-ready and ad-hoc agent systems alike, and gateable in CI through
Assay Harness or Assay GitHub Actions.

## References

- [Netlify Agent Experience](https://www.netlify.com/agent-experience/)
- [Stainless MCP servers for agentic coding](https://www.stainless.com/products/mcp)
- [Nx: Agentic Experience Is the New Developer Experience](https://nx.dev/blog/making-nx-agent-ready)
- [OpenAI: Harness engineering](https://openai.com/index/harness-engineering/)
- [Anthropic: Effective harnesses for long-running agents](https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents)
- [Anthropic: Harness design for long-running application development](https://www.anthropic.com/engineering/harness-design-long-running-apps)
- [NIST AI 600-1 Generative AI Profile](https://www.nist.gov/publications/artificial-intelligence-risk-management-framework-generative-artificial-intelligence)
- [Evidence Receipts in Action](EVIDENCE-RECEIPTS-IN-ACTION.md)
- [Receipt family matrix](../reference/receipt-family-matrix.json)
