# PLAN — P8 MCP-Agent Runtime Accounting Evidence Interop (2026 Q2)

- **Date:** 2026-04-07
- **Owner:** Evidence / Product
- **Status:** Planning
- **Scope (this PR):** Define the next external interop lane after the framework and protocol wave. No sample implementation, no outward post, no contract freeze in this slice.

## 1. Why this plan exists

After the current framework and protocol wave, the next outward lane should not
be chosen by repo popularity alone.

The better test is:

1. does the upstream project already expose one bounded runtime surface,
2. can Assay consume that surface without inheriting upstream semantics as truth,
3. does the repo have a natural maintainer channel for one small sample-backed question.

`lastmile-ai/mcp-agent` currently fits that pattern better than a broader
framework pitch:

- the repo is MCP-native in its product identity
- Discussions are enabled
- the docs expose structured observability surfaces, including token accounting

That makes it a strong next candidate, but only if Assay frames the seam
honestly.

This is **not** an MCP protocol evidence plan.

This is a plan for a **bounded runtime-accounting seam inside an MCP-native
framework**.

## 2. Hard positioning rule

This lane must not overclaim what the sample actually observes.

Normative framing:

> This sample targets the smallest honest runtime-accounting surface exposed by
> an MCP-native framework, not an MCP protocol record.

That means:

- `mcp-agent` being MCP-native is context, not the evidence seam itself
- token summaries are runtime accounting artifacts, not protocol artifacts
- Assay stays an external evidence consumer, not a billing authority, protocol
  authority, or workflow correctness authority

## 3. Why not another trace sample

`mcp-agent` also exposes tracing and observability surfaces, but repeating a
generic trace-first sample right after:

- Microsoft Agent Framework
- OpenAI Agents
- LangGraph

would make the next outward move look repetitive and less intentional.

The better first wedge is smaller and more distinct:

- token summaries
- bounded token breakdowns
- optional opaque tree references only

This gives Assay a new kind of runtime evidence sample without dragging the lane
back into full traces, watcher streams, or protocol traffic.

## 4. Recommended v1 seam

Use **token summary exported from the documented token counter path** as the
first external-consumer seam.

This seam is:

- bounded
- reviewable
- different from full traces
- already first-class in the upstream observability story
- directly aligned with the documented token counter path rather than a custom
  or inferred export shape

This is intentionally not:

- MCP packet capture
- protocol authorization evidence
- full OpenTelemetry export
- structured logs as a primary seam
- token-tree semantics as a first seam

## 5. v1 artifact contract

### 5.1 Required fields

The first sample should require:

- `schema`
- `framework`
- `surface`
- `workflow_name`
- `run_id`
- `timestamp`
- `outcome`
- `token_summary.total_tokens`
- `token_summary.input_tokens`
- `token_summary.output_tokens`

### 5.2 Optional fields

The first sample may include:

- `model_breakdown`
- `cost_estimate_usd`
- `tree_ref`

### 5.3 Important field boundaries

#### `cost_estimate_usd`

This field is optional and secondary.

If present, it must be treated as an **upstream estimate**, not as:

- billing truth
- settled cost
- procurement truth

If omitted, the sample remains fully valid.

#### `tree_ref`

`tree_ref` must stay a bounded reference only.

Allowed in v1:

- opaque reference token
- small label

Not allowed in v1:

- tree payload
- subtree semantics
- watcher events
- call-graph semantics promoted into evidence meaning

## 6. Assay-side meaning

The sample may only claim bounded runtime-accounting observation.

Assay must not treat as truth:

- token accounting correctness
- workflow correctness
- billing truth
- policy correctness
- MCP protocol correctness
- runtime semantics beyond the bounded observed artifact

Common anti-overclaim sentence:

> We are not asking Assay to inherit mcp-agent token accounting, workflow
> outcomes, or runtime semantics as truth.

## 7. Concrete repo deliverable

If this plan is accepted, the next implementation PR should add:

- `examples/mcp-agent-token-evidence/README.md`
- `examples/mcp-agent-token-evidence/requirements.txt` only if the generator truly needs it
- `examples/mcp-agent-token-evidence/generate_synthetic_run.py` only if a clean local generator is viable
- `examples/mcp-agent-token-evidence/map_to_assay.py`
- `examples/mcp-agent-token-evidence/fixtures/valid.mcp-agent.json`
- `examples/mcp-agent-token-evidence/fixtures/failure.mcp-agent.json`
- `examples/mcp-agent-token-evidence/fixtures/malformed.mcp-agent.json`
- `examples/mcp-agent-token-evidence/fixtures/valid.assay.ndjson`
- `examples/mcp-agent-token-evidence/fixtures/failure.assay.ndjson`

Fixture boundary note:

- v1 fixtures may omit `tree_ref` entirely
- v1 fixtures must not embed tree payloads

## 8. Generator policy

The implementation should prefer a real local generator **only if** it stays
small and deterministic.

### 8.1 Preferred path

Preferred:

- a local generator that exercises the documented token summary path
- no hosted dependency requirement
- no unstable network dependency
- no hidden credential requirement

### 8.2 Hard fallback rule

If a real local generator would require:

- external provider credentials
- non-deterministic network behavior
- runtime setup heavy enough to overshadow the sample

then the sample must fall back to a **docs-backed frozen artifact shape**.

The sample must not become a half-working runtime demo.

## 9. README boundary requirements

The eventual sample README must say:

- this is not a production Assay↔mcp-agent adapter
- this does not freeze a new Assay Evidence Contract event type
- this does not treat token accounting, cost estimates, or workflow outcomes as Assay truth
- this does not define an MCP protocol evidence record

## 10. Outward channel strategy

If the sample lands and the surrounding outbound queue is quiet enough, the
first outward move should be **one small Discussion** in
`lastmile-ai/mcp-agent`.

Best-fit category candidate:

- `Show and tell`

Fallback if the post reads more like a seam-direction question:

- `Ideas`

The outward question should stay narrow:

> If an external evidence consumer wants the smallest honest mcp-agent surface
> for bounded runtime accounting evidence, is `token_summary` roughly the right
> seam, or is there a thinner exported surface you'd rather point them at?

## 11. Sequencing rule

This lane should not start implementation until the current wave is settled
enough that we are not opening too many active conversations at once.

That means:

- Microsoft Agent Framework sample and follow-up must already be out
- current open threads should be allowed to sit without new nudges
- protocol lane should not be opened outward again at the same time

## 12. Non-goals

- defining an MCP protocol evidence contract
- building packet-level MCP evidence in this wave
- treating token summaries as billing truth
- turning token trees or watcher streams into the first seam
- adding a broader logging or tracing lane in the same sample

## References

- [lastmile-ai/mcp-agent](https://github.com/lastmile-ai/mcp-agent)
- [mcp-agent docs — observability](https://docs.mcp-agent.com/mcp-agent-sdk/advanced/observability)
- [mcp-agent docs — welcome](https://docs.mcp-agent.com/get-started/welcome)
- [ADR-033: OTel Trust Compiler Positioning](./ADR-033-OTel-Trust-Compiler-Positioning.md)
