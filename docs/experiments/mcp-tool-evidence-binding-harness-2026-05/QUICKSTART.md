# MCP Tool Evidence Binding Quickstart

> Assay does not detect tool poisoning. It shows what bounded evidence
> can safely connect: which MCP tool descriptions were visible, which
> tool was called, what effect was measured, and what claim is safe.

This is an **experiment-scoped** synthetic harness
(`assay.experiment.mcp_tool_evidence_binding.binding_cell.v0`). It is not
a product API, not a security scanner, and it never contacts a live MCP
server. It exists to make one question concrete and reviewable:

*When an MCP tool description is visible to the model and a tool call
produces a measured effect, what claim is safe, and what is deliberately
left unsaid?*

## Run It

```bash
cd docs/experiments/mcp-tool-evidence-binding-harness-2026-05
python3 mcp_tool_binding_harness.py --out-dir ./out --assay-commit demo
```

This emits one directory per scenario under `./out/`, each containing
the visible tool descriptions, the tool call, the measured effect when
one was observed, and a `binding-cell.json` with the claim outcome and
its non-claims.

The six committed reference outputs are indexed from
[`runs/README.md`](runs/README.md) and stored under
`runs/starter-synthetic/`. They are regenerated and compared
byte-for-byte by the harness test suite.

## Golden Example 1

Scenario: `effect_outside_declared_tool_boundary`.

The visible `read_file` tool declares a read-only boundary:
`filesystem_read:/workspace/allowed/*`. The measured effect is a write
to `/workspace/outside/hidden.txt`.

The binding cell records:

| Field | Value |
|---|---|
| `called_tool_name` | `read_file` |
| `measured_effect_kind` | `filesystem_write` |
| `effect_capture_status` | `observed` |
| `effect_within_declared_boundary` | `false` |
| `join_key` / `join_grade` | `tool_call_id` / `strong` |
| `claim_outcome` | `effect_outside_declared_tool_boundary` |

The safe read: **the measured write left the visible read-only boundary,
bound to the same `tool_call_id`.** That is the whole claim.

What it deliberately does **not** say:

- `does_not_classify_malicious_intent`
- `does_not_claim_policy_failure`
- `does_not_claim_root_cause`
- `does_not_detect_tool_poisoning`

A boundary divergence is evidence about an effect, not a verdict about
intent. The cell carries the divergence and refuses the accusation in
the same row.

## Golden Example 2

Scenario: `call_made_with_other_descriptions_visible`.

Two tools are visible to the model: `read_file` (read-only) and
`write_file` (write to `/workspace/out/*`). The model calls
`read_file`; the measured effect stays inside its boundary.

This is the [MCP-ITP](https://arxiv.org/abs/2601.07395) shape: an
influencing tool description can be co-visible without itself being
called. The binding cell records the **complete** visible set, not just
the called tool:

| Field | Value |
|---|---|
| `called_tool_name` | `read_file` |
| `co_visible_tool_names` | `["read_file", "write_file"]` |
| `effect_within_declared_boundary` | `true` |
| `claim_outcome` | `call_isolated_in_visible_context` |

The safe read: **the called tool is bound to its own description and
effect, while the full co-visible description set is preserved as
context.**

What it deliberately does **not** say:

- `does_not_claim_co_visible_description_caused_call`

Co-visibility is recorded as a fact. Causation between another visible
description and the call is **not** claimed. That is exactly the
inference the evidence cannot support, so it is named as a non-claim
rather than left ambiguous.

## All Six Scenarios

| Scenario | Claim outcome | Safe read | What it deliberately does **not** say |
|---|---|---|---|
| `benign_tool_call_bound` | `bound_tool_evidence` | Visible description, call, and measured effect align inside the declared boundary, joined by `tool_call_id`. | Does not claim the tool is safe in general. |
| `description_changed_before_call` | `description_drift` | The model-visible description digest differs from the referenced manifest before the call. | Does not claim the change was malicious or intentional. |
| `effect_outside_declared_tool_boundary` | `effect_outside_declared_tool_boundary` | A bound call produced a measured effect beyond the visible boundary. | Does not claim maliciousness, policy failure, or root cause. |
| `call_made_with_other_descriptions_visible` | `call_isolated_in_visible_context` | The called tool is bound; other visible descriptions are recorded as co-visible context. | Does not claim co-visible descriptions caused the call. |
| `description_visible_no_call` | `diagnostic_only` | A description was visible; no call and no effect followed. | Does not claim the tool had no effect in general. |
| `call_made_no_measurable_effect` | `inconclusive` | A call was observed but no effect could be measured in the capture surface. | Does not claim the call was inert. |

## What This Is

- A synthetic, runnable demonstration of bounded
  `description -> call -> effect -> claim` reading.
- A review aid where non-claims are first-class output.
- An experiment-scoped reference for the starter harness outputs.

## What This Is Not

- A tool-poisoning detector.
- An intent classifier.
- An MCP client, server, provider, or transport ranking.
- A product API or receipt family.
- A live MCP server or tunnel deployment.

For the research framing and scenario rationale, see
[`../mcp-tool-evidence-binding-2026-05.md`](../mcp-tool-evidence-binding-2026-05.md).
