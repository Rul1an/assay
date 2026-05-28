# MCP Tool Evidence Binding Research Note

> **Status:** research note. Last updated: 2026-05-28.
> This note does not define a schema, create a receipt family, dispatch
> an MCP server, or claim to detect poisoned tools. It explores what
> bounded evidence would be needed before a reviewer can connect the
> model-visible MCP tool context to a tool call and a measured runtime
> effect.

## Goal

When MCP tool descriptions are part of the model-visible context and a
tool call produces a measured system effect, what evidence can prove
which tool definitions were visible, which tool was called, and what is
the strongest safe claim if the visible context, call, and effect
diverge?

This is an evidence-binding question, not a poisoning-detection question.
The useful output is a bounded claim about the relationship between the
visible tool context, a call, and a measured effect. It is not a
maliciousness verdict.

## Context

Recent MCP security research makes tool metadata a first-class evidence
surface. MCPTox studies tool poisoning where instructions are embedded in
tool metadata before execution. MCP-ITP studies a related implicit
pattern where one visible tool description can influence a later call to
a different tool.

At the same time, OpenTelemetry now has MCP semantic conventions for MCP
spans, attributes, sessions, methods, and tool-call execution. That helps
observability systems represent MCP activity, but it does not by itself
prove which exact tool definition was visible to the model, which call
followed, or whether the measured runtime effect stayed inside the
visible tool boundary.

Assay's relevant question is narrower than detection: can the visible
tool context, the called tool, and the measured effect be retained as a
reviewable evidence chain with explicit claim bounds?

## Prerequisites

| Dependency | Status | Role |
|---|---|---|
| Observability claim classes | Reference-ready | Reuse the discipline that evidence may support only bounded claims. |
| Join contract | Reference-ready | Reuse strong/weak/diagnostic join discipline for call/effect linking. |
| Runner measured effects | Available | Provides the system-effect boundary when a delegated or local capture exists. |
| Agent-observability fidelity findings | Closed | Supplies the non-claim discipline: divergence is not maliciousness by itself. |
| Binding evidence / join receipts | Proposed | Remains a working term; this note does not promote it to a product family. |

## Evidence Chain

The candidate chain is:

```text
context_descriptor_set
  -> model_visible_tool_description_refs[]
  -> model_visible_tool_description_digests[]
  -> tool_call
       -> tool_call_id / tool_name / call_arguments_digest
       -> called_tool_manifest_digest
       -> called_tool_description_digest
       -> policy_decision_ref (if present)
       -> measured_runtime_effect
```

The plural `context_descriptor_set` is load-bearing. MCP tool context is
not always one called tool plus one description. A description can be
visible without being called, and another tool can be called while that
description remains visible. The evidence chain must therefore preserve
the set of model-visible tool definitions at call time, not only the
definition of the called tool.

The note intentionally separates `called_tool_manifest_digest` from
`called_tool_description_digest`, and separates both from the full
`model_visible_tool_description_digests[]` set. A server manifest can
contain fields that are not rendered to the model, and a client or
framework can transform a tool definition before it reaches model
context. The model-visible digest set is therefore the load-bearing link
for visible-context binding.

Each `model_visible_tool_description_ref` should point to the exact
captured input surface when available. It may be the raw MCP tool
description, a rendered provider/tool block, or a framework-specific
model-context record. A future harness must record which surface was
used; it must not silently treat the server manifest as equivalent to the
model-visible description.

## MVP Scenarios

| Scenario | Role | Expected claim outcome | Purpose |
|---|---|---|---|
| `benign_tool_call_bound` | baseline | `bound_tool_evidence` | The visible context, called tool definition, tool call, and measured effect align inside the declared boundary. |
| `description_changed_before_call` | drift | `description_drift` | The visible description digest differs from the referenced manifest before the call. |
| `effect_outside_declared_tool_boundary` | gap | `effect_outside_declared_tool_boundary` | The call is bound to the visible definition, but the measured effect exceeds the declared visible boundary. |
| `description_visible_no_call` | absence boundary | `diagnostic_only` | A tool definition is visible in context, but no call to that tool is observed inside the bounded call surface. |
| `call_made_no_measurable_effect` | effect boundary | `diagnostic_only` or `inconclusive` | A visible definition and call exist, but the measured effect is absent, unavailable, or outside the capture surface. |
| `call_made_with_other_descriptions_visible` | context boundary | `call_isolated_in_visible_context` | A tool is called while other tool descriptions are also visible; the output records co-visible definitions without claiming causation. |

The absence-boundary scenarios are included to keep negative claims
separate. `description_visible_no_call` is about the call surface.
`call_made_no_measurable_effect` is about the measured-effect surface.
Neither should be read as "nothing happened" unless the relevant capture
layer, health gates, and declared measurement surface can support that
narrow claim.

The co-visible context scenario is included to cover implicit influence
patterns without claiming influence. It can say which other tool
definitions were visible when a call was made. It cannot say that those
definitions caused the call.

## Safe Claim Outcomes

| Claim outcome | Meaning |
|---|---|
| `bound_tool_evidence` | The visible tool context, called tool definition, observed tool call, and measured effect can be linked inside the bounded evidence chain. |
| `description_drift` | The model-visible tool description changed relative to the expected or referenced definition before the call. |
| `effect_outside_declared_tool_boundary` | The measured runtime effect exceeds the declared or visible tool boundary without proving malicious intent. |
| `call_isolated_in_visible_context` | The called tool and the complete set of co-visible tool definitions can be recorded for the call without making a causation claim. |
| `diagnostic_only` | The required links are present, but the chain supports only a descriptive or bounded negative claim. |
| `inconclusive` | One or more required links are missing, unverifiable, or health-bounded, so the intended chain cannot be interpreted. |

These outcomes are research-note vocabulary, not a schema enum. A future
harness PR may choose a schema shape only after the synthetic evidence
chain is reviewed.

## Acceptance Rules

1. A future harness must preserve the complete model-visible tool
   description set for the call, or a digest list plus source references
   to that set.
2. `called_tool_manifest_digest` may not substitute for
   `called_tool_description_digest`, and neither may substitute for the
   full visible description set, unless the harness proves
   byte-equivalence for that scenario.
3. A strong call/effect claim requires a concrete join key such as
   `tool_call_id`; name-only joins are diagnostic unless the scenario
   explicitly bounds the ambiguity.
4. Runtime effects must remain measured-effect evidence. They may not be
   upgraded into model intent without a visible description and call
   link.
5. Description/effect divergence may support
   `effect_outside_declared_tool_boundary`, but not a maliciousness
   verdict.
6. An absent measured effect may support only a bounded negative claim
   inside the recorded capture layer, health state, configured limits,
   and measurement method.
7. Synthetic fixtures would be acceptable for a future first harness
   gate. A delegated MCP capture would require a separate delegated-gate
   plan.
8. No schema, receipt family, or product API is promoted by this note.

## Possible Future Harness Gate

A follow-up harness PR is review-ready when it can emit stable synthetic
outputs for the MVP scenarios with:

- one captured or fixture-backed model-visible description set per
  scenario;
- digests for the visible description set, called-tool manifest, and
  called-tool description;
- a tool-call record with a stable call key when the scenario requires a
  strong join;
- a measured-effect record or an explicit `not_applicable`/unobserved
  capture boundary;
- one claim outcome per scenario;
- non-claims attached to each scenario output.

The starter harness should be local and synthetic. It should not contact
live MCP servers, rank MCP implementations, or publish delegated security
findings.

## Non-Claims

- This note does not attempt to detect poisoned tools.
- This note does not classify malicious intent.
- This note does not prove that a tool description was safe.
- This note does not rank MCP clients, servers, providers, or
  observability stacks.
- This note does not define MCP specification changes.
- This note does not require production MCP deployment.
- This note does not treat a measured effect divergence as proof of
  attack.
- This note does not create a new receipt family or promote binding
  evidence to a product surface.
- This note does not publish outreach targets, comment drafts, or private
  sequencing notes.

## Source Anchors

- MCPTox benchmark:
  <https://arxiv.org/abs/2508.14925>
- MCP-ITP implicit tool poisoning framework:
  <https://arxiv.org/abs/2601.07395>
- OpenTelemetry semantic conventions for MCP:
  <https://opentelemetry.io/docs/specs/semconv/gen-ai/mcp/>
- Agent-observability fidelity findings:
  [`agent-observability-fidelity-2026-05/findings-summary.md`](agent-observability-fidelity-2026-05/findings-summary.md)
- Observability claim classes:
  [`../reference/observability/claim-classes-v0.md`](../reference/observability/claim-classes-v0.md)
- Observability join contract:
  [`../reference/observability/join-contract-v0.md`](../reference/observability/join-contract-v0.md)
