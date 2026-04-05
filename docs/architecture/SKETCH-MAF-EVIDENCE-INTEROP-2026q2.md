# Sketch: Microsoft Agent Framework Trace Evidence Interop

Date: 2026-04-06
Status: v1 sketch only

## Purpose

This note sketches the smallest useful interop sample between Assay and
Microsoft Agent Framework.

It is intentionally narrow. It is not a roadmap commitment, a partnership
announcement, or a request for Agent Framework to grow a new governance
feature.

The goal is simpler:

- let Agent Framework keep doing runtime orchestration and observability
- let Assay keep compiling bounded external evidence
- test one honest handoff between them

## Current read

Agent Framework 1.0 now looks strongest where it is already explicit:

- production-ready framework APIs
- built-in observability
- protocol-facing interoperability such as MCP and A2A

Assay looks strongest when it takes upstream runtime output and turns it into
portable, reviewable evidence:

- deterministic evidence bundles
- Trust Basis
- Trust Card
- CI-facing outputs such as SARIF

The overlap is real, but the products do not need to become each other.

## Recommended v1 seam

Use **Agent Framework exported observability traces** as the first interop
surface.

More specifically:

- start with one small exported run trace or span set
- map only a bounded subset into Assay evidence
- treat the rest as out of scope for v1

This is a better first seam than a broader "audit feature" ask because Agent
Framework already presents observability as part of the product. The question
is not "can you add governance output for us?" It is "is there a smallest
stable trace surface an external evidence consumer can safely read?"

## Example input

The v1 sample should use one tiny exported trace record. The exact field names
should follow whatever Agent Framework already treats as stable for external
consumers.

Illustrative shape:

```json
{
  "trace_id": "4bf92f3577b34da6a3ce929d0e0e4736",
  "span_id": "00f067aa0ba902b7",
  "name": "agent.run",
  "start_time": "2026-04-06T10:14:23.120Z",
  "end_time": "2026-04-06T10:14:24.002Z",
  "status": "OK",
  "attributes": {
    "agent.id": "assistant-1",
    "run.id": "run_42",
    "tool.name": "web_search"
  }
}
```

This example is intentionally small. It is just enough to test whether Assay
can consume a bounded trace surface without pretending to own Agent Framework's
runtime semantics.

## Minimal Assay mapping

Assay should treat this as **external runtime trace evidence**.

Suggested imported shape:

```json
{
  "kind": "external.runtime.trace",
  "source": "microsoft:agent-framework",
  "observed": {
    "trace_id": "4bf92f3577b34da6a3ce929d0e0e4736",
    "span_id": "00f067aa0ba902b7",
    "name": "agent.run",
    "status": "OK",
    "agent_id": "assistant-1",
    "run_id": "run_42",
    "tool_name": "web_search",
    "start_time": "2026-04-06T10:14:23.120Z",
    "end_time": "2026-04-06T10:14:24.002Z"
  }
}
```

The important thing is not the exact event shape. The important thing is that
Assay stays honest about what it observed.

## What stays observed

In v1, Assay should keep the imported Agent Framework signal in the observed
bucket:

- trace identity
- run identity
- tool identity
- timestamps
- span status
- framework-emitted attributes

## What Assay should not import as truth

We are not asking to import Agent Framework runtime judgments, policy meaning,
or higher-level orchestration semantics into Assay as truth. We are asking
whether there is a smallest stable output surface that can be compiled into
bounded external evidence.

That means v1 should explicitly avoid:

- semantic translation of framework-specific run meaning
- any score or tier mapping into Assay trust language
- claims that Assay independently verified runtime correctness
- claims that a successful span implies safe behavior

## Why this helps

- it gives Assay a real framework-native trace corpus instead of toy examples
- it gives Agent Framework a portable evidence path without changing its
  runtime model
- it keeps the interop seam small enough to discuss without turning into a
  platform merger story

## External ask

If Agent Framework maintainers engage, the best next ask is still small:

- point to one smallest stable exported trace or run-output surface for
  external consumers
- provide one tiny sample artifact
- confirm which fields are intentionally stable enough to consume

That is enough to decide whether a real interop sample is worth building.
