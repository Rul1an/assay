# What Assay Is And Is Not

Assay compiles agent runtime signals and selected external outcomes into
verifiable evidence and bounded Trust Basis claims.

It is strongest when a team needs deterministic governance over tool calls,
portable evidence bundles, and reviewable trust artifacts in CI. It is not an
eval runner, observability dashboard, compliance oracle, or general-purpose
authorization service.

## Core Boundary

Assay owns this chain:

```text
runtime/import signal
  -> canonical evidence bundle
  -> bundle verification
  -> Trust Basis claims
  -> Trust Card / SARIF / CI projections
```

Policy enforcement is still a key wedge. Assay can sit between an agent and MCP
tools, evaluate explicit policy, and record the decision. The broader product
surface is the evidence compiler around those decisions: what happened, what was
verified, what was merely visible, and what should not be claimed.

## In Scope

| Area | What Assay Does |
|---|---|
| Protocol policy | Deterministic allow/deny/approval decisions over supported MCP tool-call surfaces. |
| Evidence bundles | Offline-verifiable evidence artifacts with canonical event envelopes and content binding. |
| Trust Basis | Bounded claim classification from verified bundles, keyed by stable `claim.id`. |
| Trust Card | Human-readable JSON/Markdown projection of the Trust Basis claim set. |
| External receipts | Narrow compiler lanes for selected upstream seams such as Promptfoo assertion components, OpenFeature boolean `EvaluationDetails`, and CycloneDX ML-BOM model components. |
| CI projections | SARIF/JUnit/Markdown outputs where appropriate, with raw canonical artifacts kept separate. |
| Packs | Optional evidence linting and policy packs that structure findings; packs do not prove legal compliance by themselves. |

The machine-readable receipt family surface is tracked in the
[receipt family matrix](../reference/receipt-family-matrix.json).

## Out Of Scope

| Area | Why It Is Not Assay |
|---|---|
| Eval running | Promptfoo, DeepEval, Braintrust, LangSmith, Langfuse, Phoenix, and similar tools should run or manage evaluations. Assay imports selected outcomes as bounded receipts when useful. |
| Observability dashboard | Assay can export or bridge evidence, but it does not replace tracing, metrics, prompt management, or production monitoring platforms. |
| Trust score | Trust Basis claims use explicit evidence levels. Assay does not collapse trust into a single score, badge, or "safe/unsafe" label. |
| Compliance certification | Assay can produce evidence and pack findings. It does not certify EU AI Act, SOC 2, or other legal compliance. |
| Full BOM viewer | CycloneDX ML-BOM receipts preserve selected inventory boundaries. Assay does not import full BOM graphs, vulnerabilities, licenses, or model-card truth. |
| Semantic safety classifier | Toxicity, jailbreak, hallucination, bias, and content-safety checks require probabilistic or model-based systems. Assay should complement them, not impersonate them. |

## Assay Versus Assay Harness

Assay owns artifact semantics:

- evidence import and reduction,
- evidence bundle verification,
- receipt schemas and receipt-family matrix,
- Trust Basis generation and diff semantics,
- Trust Card generation.

Assay Harness owns operational CI recipes above those artifacts:

- run baseline/candidate recipes,
- preserve raw Assay diff JSON,
- map Trust Basis regressions to CI exits,
- project raw diffs into Markdown or JUnit.

Harness must not parse Promptfoo JSONL, OpenFeature JSONL, CycloneDX BOMs, or
Assay receipt payloads. Domain semantics stay in Assay.

## Decision Rule

Use Assay when the answer should come from deterministic policy, verified
evidence, or a bounded receipt boundary.

Use another tool first when the answer requires subjective scoring, semantic
judgment, broad trace exploration, prompt iteration, or legal certification.

Assay should make those external results portable when they matter; it should
not become those systems.
