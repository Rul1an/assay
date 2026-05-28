# MCP Tool-Evidence Comment Drafts

> **Status:** outreach draft packet. Last updated: 2026-05-28.
> This document does not open an Assay experiment arc, define a schema,
> promise an evidence-pack product, or promote any experiment-scoped
> artifact to a product API.

## Purpose

This packet turns the post-arc claim-boundary position into three
upstream-ready comments for MCP tool poisoning and tool-description
poisoning work. The goal is to offer Assay's bounded-evidence vocabulary
to benchmark and specification discussions without committing Assay to a
new implementation arc.

The shared evidence shape is:

```text
tool_manifest_digest
  -> model_visible_description
  -> tool_call
  -> runtime_effect
```

The shape is intentionally claim-bound. It asks what evidence would let a
reviewer say:

- which tool metadata was visible to the model;
- which tool call the model selected and with which arguments;
- which policy or authorization context was in force, if any;
- which runtime effect was measured; and
- which claim is safe: positive join, semantic gap, diagnostic only, or
  inconclusive.

It does not claim that every poisoned description is malicious, that
every mismatch is an exploit, or that Assay should become an MCP scanner.

## Source Anchors

- MCP tools specification, version `2025-06-18`:
  <https://modelcontextprotocol.io/specification/2025-06-18/server/tools>
- MCPTox, AAAI-26:
  <https://ojs.aaai.org/index.php/AAAI/article/view/40895>
- MCP-TDP, arXiv `2605.24069`, submitted 2026-05-22:
  <https://arxiv.org/abs/2605.24069>
- Assay claim-boundary positioning:
  [`claim-boundary-positioning.md`](claim-boundary-positioning.md)
- Assay agent-observability fidelity findings:
  [`../../experiments/agent-observability-fidelity-2026-05/findings-summary.md`](../../experiments/agent-observability-fidelity-2026-05/findings-summary.md)

## Shared Comment Core

Use this core in all three upstream comments, then adapt the opening and
closing paragraph to the target.

```markdown
One adjacent evidence question this work raises is not just "did the
agent fall for a poisoned tool description?", but "which bounded claim
can a reviewer make after the run?"

The evidence shape I would like to see made explicit is:

`tool_manifest_digest -> model_visible_description -> tool_call -> runtime_effect`

That shape separates four layers:

- the tool metadata that was actually visible to the model;
- the selected tool call and arguments;
- any policy, authorization, or human-approval context in force;
- the measured runtime effect or externally visible action.

The useful output is not a stronger claim by default. It is a bounded
claim class:

- `positive_join`: metadata, call, and measured effect line up;
- `semantic_gap`: reported tool intent and measured effect diverge under
  a strong join key;
- `diagnostic_only`: the evidence only joins by weak keys such as
  timestamp/order or run id;
- `inconclusive`: health, calibration, redaction, or join evidence is not
  strong enough to cite the run as a poisoning finding.

This framing keeps tool-poisoning evidence reviewable without assuming
that every mismatch is malicious, every bad outcome is caused by the
tool description, or every absent field proves absent behavior.
```

## Draft 1: MCPTox

Target: MCPTox paper, code release, discussion, or benchmark feedback
thread.

```markdown
Thanks for publishing MCPTox. The benchmark's focus on tool poisoning at
the metadata/registration layer is a useful distinction from attacks that
arrive through tool outputs after a call has already happened.

One adjacent evidence question this work raises is not just "did the
agent fall for a poisoned tool description?", but "which bounded claim
can a reviewer make after the run?"

The evidence shape I would like to see made explicit is:

`tool_manifest_digest -> model_visible_description -> tool_call -> runtime_effect`

That shape separates four layers:

- the exact tool metadata/description visible to the model;
- the selected tool call and arguments;
- any policy, authorization, or human-approval context in force;
- the measured runtime effect or externally visible action.

For a benchmark like MCPTox, this would make failure analysis more
portable. A case where a model selects a poisoned tool and the measured
effect matches the attack objective is different from a case where the
model selects the tool but the effect is blocked, rewritten, or only
weakly attributable to that tool call.

The useful output is a bounded claim class rather than a stronger claim
by default:

- `positive_join`: metadata, call, and measured effect line up;
- `semantic_gap`: reported tool intent and measured effect diverge under
  a strong join key;
- `diagnostic_only`: the evidence only joins by weak keys such as
  timestamp/order or run id;
- `inconclusive`: health, calibration, redaction, or join evidence is not
  strong enough to cite the run as a poisoning finding.

This would not require MCPTox to adopt any specific Assay format. The
smallest useful addition would be an optional per-case evidence row that
records the tool manifest digest, model-visible description hash or
snapshot, tool-call id/name/arguments, measured effect class, join key,
and maximum safe claim.
```

## Draft 2: MCP-TDP

Target: MCP-TDP benchmark discussion, artifact feedback, or follow-up
issue.

```markdown
The "manual lies" framing is a sharp way to describe Tool Description
Poisoning: the attack lives in the descriptive metadata the agent uses
for planning, not necessarily in executable tool code.

One evidence addition that would make MCP-TDP results easier to compare
across agents is a bounded per-case claim record:

`tool_manifest_digest -> model_visible_description -> tool_call -> runtime_effect`

For TDP, I would especially want the record to distinguish:

- the poisoned description that was visible to the model at planning
  time;
- the call the model actually made;
- whether a defense or self-correction step changed the eventual effect;
- the measured runtime effect or externally visible action; and
- whether the join between call and effect is strong, weak, or absent.

That distinction matters for reactive self-correction cases. "The model
initially chose the poisoned path and later reverted it" is not the same
claim as "the final system state reflects the poisoned objective." Both
are useful, but they should not collapse into a single attack-success
label without the evidence boundary being visible.

The maximum safe claim could be represented as:

- `semantic_gap` when the model-visible description/call and measured
  effect diverge under a strong join;
- `diagnostic_only` when the case depends on timing/order proximity or
  other weak evidence;
- `inconclusive` when logs, health, redaction, or join data are not
  strong enough to cite a poisoning finding.

This is not a request to rank defenses or impose a new schema. It is a
small reviewability layer that makes benchmark cases easier to audit
when tool descriptions, calls, defenses, and runtime effects do not all
tell the same story.
```

## Draft 3: MCP Tools Specification

Target: MCP tools specification discussion or issue.

```markdown
The tools specification already treats tool descriptions and annotations
as model-facing inputs with trust and safety implications. Recent
tool-poisoning work suggests one possible specification-adjacent
question: what evidence should an implementation preserve so a reviewer
can later bound claims about a tool invocation?

I do not think the spec needs to mandate a runtime audit format. A
smaller interoperability hook would be enough: document the evidence
boundary around tool metadata visibility and tool invocation.

The reviewable shape is:

`tool_manifest_digest -> model_visible_description -> tool_call -> runtime_effect`

Concretely, clients or hosts could be encouraged to preserve enough
information to answer:

- Which tool definition was visible to the model when it selected a
  tool?
- Was that definition identified by a digest or stable snapshot?
- Which `tools/call` request was made, with which name and arguments?
- Which result or runtime effect can be joined back to that call?
- Was the join strong, weak, diagnostic-only, or inconclusive?

This would help downstream benchmark and safety tooling distinguish
"the model saw this description and called this tool" from stronger
claims such as "this description caused that system effect." It would
also avoid treating absent trace fields as proof that no behavior
occurred.

This suggestion is intentionally not a proposal for MCP to adopt Assay,
define a new product API, or rank tool-host implementations. It is a
specification note about preserving enough evidence for bounded claims
when tool metadata becomes part of the model's decision surface.
```

## Posting Rules

Before posting any comment:

1. Prefer the shortest target-specific draft that still names the
   evidence boundary.
2. Remove Assay-specific vocabulary if it distracts from the upstream
   discussion, but keep the claim-bound structure.
3. Do not promise a schema, evidence pack, importer, delegated
   experiment, or product feature.
4. Do not frame the comment as "Assay can solve this." Frame it as
   "this evidence boundary may make findings easier to audit."
5. If a maintainer asks for a concrete artifact shape, that is the
   trigger to open a small Assay planning issue or experiment arc.

## Non-Claims

- This packet does not assert that MCPTox or MCP-TDP lack evidence.
- This packet does not claim that MCP tool metadata poisoning always
  causes malicious runtime behavior.
- This packet does not define `tool_manifest_digest` as a canonical
  Assay schema field.
- This packet does not promote binding evidence, join receipts, or
  evidence packs to product APIs.
- This packet does not require MCP implementations to emit Runner,
  OpenTelemetry, OpenInference, or Assay artifacts.
