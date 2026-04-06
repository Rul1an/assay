# Sketch: CrewAI Event Evidence Sample

Date: 2026-04-06
Status: v1 sketch only

## Purpose

This note sketches the smallest useful Assay x CrewAI path for the current
round.

It is intentionally not a broad integration plan.

CrewAI is large, fast-moving, and already rich in orchestration, tooling, and
telemetry surfaces. For that reason, the best first move is not a general
"should we integrate?" discussion. The best first move is one small sample
that demonstrates an honest handoff.

The goal is simple:

- let CrewAI keep doing orchestration and tool execution
- let Assay keep compiling bounded external evidence
- use one tiny exported artifact instead of asking CrewAI to become a
  governance product

## Current read

CrewAI already exposes several surfaces that matter here:

- local event listeners and an event bus
- task, tool, and evaluation events
- MCP server support, including local stdio and remote transports
- richer tracing through CrewAI AMP

The strongest v1 seam is not AMP tracing.

AMP may become useful later, but it adds hosted product assumptions and wider
semantics than we need for a first sample. For v1, the cleaner route is to use
CrewAI's local event system and export a small artifact that Assay can consume
without inheriting CrewAI's own product semantics.

## Recommended v1 seam

Use **one local event-listener export** as the first Assay x CrewAI sample.

More specifically:

- register a custom `BaseEventListener`
- emit a tiny NDJSON artifact during one CrewAI run
- keep the exported shape deliberately small
- prefer task, tool, and MCP-adjacent execution events over broad tracing data

This is a better first step than opening with a large repo discussion because
it gives us something concrete to show.

## Recommended sample shape

The first sample should use one short CrewAI run and export only a bounded set
of events.

Preferred event family:

- `CrewKickoffStartedEvent`
- `CrewKickoffCompletedEvent`
- `TaskStartedEvent`
- `TaskCompletedEvent`
- `TaskFailedEvent`
- `ToolUsageStartedEvent`
- `ToolUsageFinishedEvent`
- `ToolUsageErrorEvent`

If the sample uses CrewAI's MCP tooling, it is worth preferring the MCP-flavored
tool execution events when they are available.

The key point is not to export everything. The key point is to export one
reviewable run artifact with enough structure to prove the evidence handoff is
real.

## Example input artifact

Illustrative NDJSON shape:

```ndjson
{"event_type":"CrewKickoffStartedEvent","timestamp":"2026-04-06T10:14:23Z","crew_name":"research_crew","run_id":"run_42"}
{"event_type":"TaskStartedEvent","timestamp":"2026-04-06T10:14:24Z","run_id":"run_42","task_id":"task_1","agent_role":"researcher"}
{"event_type":"ToolUsageFinishedEvent","timestamp":"2026-04-06T10:14:25Z","run_id":"run_42","task_id":"task_1","tool_name":"web_search","status":"ok","duration_ms":83}
{"event_type":"TaskCompletedEvent","timestamp":"2026-04-06T10:14:26Z","run_id":"run_42","task_id":"task_1","output_hash":"sha256:8aa2..."}
{"event_type":"CrewKickoffCompletedEvent","timestamp":"2026-04-06T10:14:27Z","crew_name":"research_crew","run_id":"run_42","status":"ok"}
```

This is intentionally tiny. It is enough to test:

- one successful run
- one observed tool execution
- one output binding token
- one stable exported artifact

The first corpus should also include:

- one tool or task failure case
- one malformed record case

That gives Assay a realistic sample without turning the sample into a platform
project.

## Minimal Assay mapping

Assay should treat the exported file as **external runtime evidence**.

Suggested imported evidence shape (ADR-006-style, abbreviated envelope).
The `type` value below is a sketch-only placeholder, not a registered Evidence
Contract event type:

```json
{
  "specversion": "1.0",
  "type": "example.placeholder.external-runtime-event",
  "source": "crewai:event-listener",
  "time": "2026-04-06T10:14:30Z",
  "data": {
    "event_type": "ToolUsageFinishedEvent",
    "upstream_timestamp": "2026-04-06T10:14:25Z",
    "run_id": "run_42",
    "task_id": "task_1",
    "agent_role": "researcher",
    "tool_name": "web_search",
    "status": "ok",
    "duration_ms": 83,
    "output_hash": null
  }
}
```

Envelope `time` should be the Assay import timestamp. The CrewAI event
timestamp stays observed upstream metadata.

## What stays observed

For v1, Assay should keep imported CrewAI data in the observed bucket:

- event type
- run, task, and agent identifiers
- tool or MCP server identity
- timestamps
- status strings
- output hashes
- raw error strings
- any evaluation or score fields if they appear in later samples

## What Assay should not import as truth

We are not asking Assay to import CrewAI runtime semantics, agent reasoning, or
evaluator judgments as truth.

That means the first sample should explicitly avoid:

- importing prompt traces as authoritative truth
- translating CrewAI evaluation scores into Assay trust language
- implying that Assay independently verified task correctness
- implying that a completed CrewAI run means the system was safe or compliant

If `TaskEvaluationEvent` or later tracing metrics appear, they should remain
observed evidence only.

## Why a sample-first strategy is the right approach

This route is different from the Microsoft Agent Framework and Google ADK
tracks.

For CrewAI, the better first move is a sample-first path:

- the repo is large and busy
- local hooks already exist
- a tangible sample will travel better than an abstract integration request

So the right order is:

1. build one tiny local sample
2. freeze one tiny artifact corpus
3. show the sample publicly
4. only then ask whether CrewAI wants to point to a preferred stable surface

## Concrete preparation plan

Preparation for the sample should stay small:

1. Build a minimal CrewAI example with a custom `BaseEventListener`.
2. Export a single NDJSON artifact from one short run.
3. Include one success case, one failure case, and one malformed artifact case.
4. Map the artifact into bounded Assay evidence without importing CrewAI truth.
5. Use that sample as the basis for any later CrewAI outreach or show-and-tell.

## Next external ask

If and only if the sample is clean, the next public ask should also stay
small:

- point to one preferred stable event or audit export surface for external
  consumers
- confirm whether CrewAI wants local event-listener exports or another artifact
  shape to be the recommended path

That keeps the conversation grounded in something real.
