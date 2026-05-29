# MCP Tool Evidence Binding Harness

> **Status:** synthetic harness-ready. Last updated: 2026-05-29.
> This harness does not contact live MCP servers, deploy MCP tunnels,
> detect poisoned tools, classify maliciousness, rank MCP
> implementations, or promote a product schema. It emits local synthetic
> rows that test whether a visible MCP tool context, tool call, and
> measured runtime effect can be bound into bounded claim outcomes.

## Goal

Turn the MCP tool evidence-binding research note into a small synthetic
review gate. The harness asks whether each starter scenario can preserve:

- the complete model-visible tool description set;
- the called-tool manifest and called-tool visible description digests;
- a concrete tool-call record when a call exists;
- a measured effect or explicit unobserved/unavailable boundary;
- one safe claim outcome and attached non-claims.

## Starter Scenarios

| Scenario | Expected claim outcome | Purpose |
|---|---|---|
| `benign_tool_call_bound` | `bound_tool_evidence` | Visible description, call, and measured effect align inside the declared boundary. |
| `description_changed_before_call` | `description_drift` | The visible description differs from the referenced manifest before the call. |
| `effect_outside_declared_tool_boundary` | `effect_outside_declared_tool_boundary` | The measured effect exceeds the visible declared tool boundary without proving intent. |
| `description_visible_no_call` | `diagnostic_only` | A tool definition is visible, but no call to that tool is observed. |
| `call_made_no_measurable_effect` | `inconclusive` | A call exists, but the measured-effect layer is unavailable. |
| `call_made_with_other_descriptions_visible` | `call_isolated_in_visible_context` | A called tool is bound while preserving other co-visible tool descriptions without claiming causation. |

## Tunnel Boundary

The `benign_tool_call_bound` fixture includes a synthetic MCP tunnel
transport context. This mirrors private-network/tunnel deployments where
a tunnel/proxy path can be relevant provenance, but it is not evidence of
tool intent and does not authenticate the upstream MCP server by itself.
The row therefore records `transport_claim: transport_context_only` and
non-claims for tunnel routing and upstream authentication.

This keeps transport evidence separate from the core binding question:
which tool descriptions were visible, which tool was called, and what
runtime effect was measured.

## Output Layout

Run:

```bash
python3 mcp_tool_binding_harness.py --out-dir /tmp/mcp-tool-binding-runs
```

Each scenario directory contains:

- `binding-cell.json` — the schema-validated bounded claim row;
- `context-descriptor-set.json` — the complete synthetic visible tool
  description set;
- `tool-call.json` when a call exists;
- `measured-effect.json` when an effect is observed;
- `transport-context.json` only for the synthetic tunnel fixture;
- `summary.md`.

The checked-in starter outputs are indexed from
[`runs/README.md`](runs/README.md). The harness tests regenerate the
`runs/starter-synthetic/` directory and fail if the committed outputs
drift from the generator.

## Non-Claims

- This harness does not detect tool poisoning.
- This harness does not classify malicious intent.
- This harness does not claim co-visible descriptions caused a call.
- This harness does not treat tunnel routing as tool intent.
- This harness does not deploy or test a real MCP tunnel.
- This harness does not create a receipt family or product API.
