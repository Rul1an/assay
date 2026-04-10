# PLAN — P16 LiveKit Agents Testing-Result / RunEvent Evidence Interop (2026 Q2)

- **Date:** 2026-04-10
- **Owner:** Evidence / Product
- **Status:** Planning
- **Scope (this PR):** Define the next LiveKit Agents interop lane after the
  current Browser Use, Langfuse, Mastra, and x402 wave. Include a small sample
  implementation, with no outward community post, no outward GitHub issue, and
  no contract freeze in this slice.

## 1. Why this plan exists

After the current wave, the next lane should still pass the same three tests:

1. the upstream project already exposes one bounded surface,
2. Assay can consume that surface without inheriting upstream semantics as
   truth,
3. the upstream project has at least one natural maintainer or community
   channel for one small sample-backed boundary question.

`livekit/agents` fits that pattern well enough to justify a formal plan:

- the repo is large, current, and actively changing
- the public docs expose a small official testing surface through
  `voice.testing.RunResult`
- that surface already documents typed turn events such as `message`,
  `function_call`, `function_call_output`, and `agent_handoff`
- the same docs position this namespace as a testing and evaluation surface,
  not as a generic production export or telemetry stream
- the repo points technical discussion toward the LiveKit community, which is a
  stronger fit for a seam question than forcing the first outreach through a
  GitHub feature request

This is **not** a telemetry export plan.

This is **not** a session report plan.

This is **not** a room metrics plan.

This is **not** a transcript export plan.

This is **not** a raw audio plan.

This is a plan for a **bounded artifact derived from the documented
`voice.testing.RunResult` testing-result surface**.

## 2. Why LiveKit Agents is a good `P16` candidate

LiveKit sits in a useful position in the current queue:

- it opens a new runtime class after the current protocol-first `P15 x402`
  lane
- it stays close to agent behavior and orchestration without collapsing back
  into generic traces or dashboard exports
- it has a clearly documented result surface that is smaller and more honest
  for Assay than its broader runtime, community, and deployment story

That makes LiveKit a stronger `P16` than another platform-adjacent
observability lane like `OpenLIT`.

Why:

- `OpenLIT` is more likely to read as platform-on-platform outreach
- LiveKit's testing utilities already expose an event/result shape that is
  easier to consume as bounded external evidence
- LiveKit gives Assay a voice/realtime-adjacent lane without making the first
  wedge about audio infrastructure

At the same time, the channel shape is different from Agno or Browser Use:

- `livekit/agents` has no Discussions
- GitHub blank issues are disabled
- GitHub issue templates are oriented around bugs and feature requests
- the repo points technical conversation toward the LiveKit community

That means `P16` should be sample-first and **community-first**, not
GitHub-issue-first.

## 3. Hard positioning rule

This lane must not overclaim what the sample actually observes.

Normative framing:

> This sample targets the smallest honest LiveKit Agents testing-result
> surface exposed through `voice.testing.RunResult` and typed `RunEvent`s, not
> production telemetry, session analytics, room-state observability, or
> runtime correctness truth.

That means:

- LiveKit Agents is the upstream runtime context, not the truth source
- `voice.testing.RunResult` is a test/result surface, not a production
  observability export contract
- typed `RunEvent`s are bounded observations inside a test turn, not proof of
  workflow correctness or call success beyond the observed artifact
- Assay stays an external evidence consumer, not a judge of room correctness,
  audio quality, handoff correctness, or realtime runtime correctness

## 4. Why not telemetry-first

LiveKit makes it very tempting to start with telemetry, runtime state, or
session analytics because the broader product and docs also discuss:

- realtime sessions and rooms
- operational monitoring
- deployment and runtime infrastructure
- broader production agent behavior

That would be the wrong first wedge.

Why:

- it would make the lane look too much like another observability integration
- it would skip the smaller official testing surface already documented in
  `voice.testing`
- it would invite overclaiming around session quality, call success, or
  runtime truth
- it would turn the sample into infrastructure theater instead of a small
  external-consumer seam

The cleaner first wedge is:

- one artifact derived from `RunResult.events`
- one bounded list of typed turn events
- one optional `final_output_ref` only if naturally present
- no room metrics
- no session reports
- no traces
- no runtime deployment metadata

## 5. Why not transcript-first

Voice agents naturally make transcripts and speech payloads tempting.

That would still be the wrong first wedge.

Why:

- transcript dumps are much larger than the minimum honest seam
- they quickly turn the lane into a conversation export instead of a test
  result export
- they raise privacy and reviewability pressure that the first slice does not
  need
- they blur the distinction between small typed event evidence and raw session
  content

So for v1:

- `message.content` must stay short and bounded
- no multi-turn transcript dump belongs in one event
- no audio blobs, no chunk arrays, and no raw speech payloads belong in the
  sample

## 6. Why events-first, not final-output-first

`RunResult` exposes more than one useful surface:

- `events`
- final output
- assertion helpers around those events

The first seam should still be **events-first**.

Why:

- the docs describe `events` as the ordered record of what happened during the
  run
- typed events are smaller and more reviewable than a large final output blob
- `finalOutput` in the JS docs is currently weaker than the event story, while
  Python also makes clear that final output only exists when present at the end
  of the run
- event-first keeps the lane distinct from Browser Use, which already leans
  toward final-result and action-history style output

That means:

- `events` are the primary seam
- `final_output_ref` is optional bonus context only
- the sample must remain complete and honest without any final output field

## 7. Recommended v1 seam

Use **one frozen serialized artifact derived from the documented
`voice.testing.RunResult` testing-result surface** as the first external
consumer seam.

Primary seam:

- `events`

Secondary seam:

- `final_output_ref` only if naturally present in the chosen frozen artifact

Allowed v1 event types:

- `message`
- `function_call`
- `function_call_output`
- `agent_handoff`

Important framing rule:

> The sample uses a frozen serialized artifact derived from
> `voice.testing.RunResult.events`, not a claim that LiveKit already guarantees
> one fixed wire-export contract for external evidence consumers.

## 8. v1 artifact contract

### 8.1 Required fields

The first sample should require:

- `schema`
- `framework`
- `surface`
- `runtime_mode`
- `task_label`
- `timestamp`
- `outcome`
- `events`

Default values for the frozen sample shape:

- `framework = livekit_agents`
- `surface = voice_testing_run_result`
- `runtime_mode = voice.testing`

### 8.2 Optional fields

The first sample may include:

- `final_output_ref`
- `agent_ref`
- `error_label`
- `sdk_version_ref`

### 8.3 Top-level validation posture

The mapper should stay strict on the bounded seam itself while remaining
future-tolerant toward unrelated top-level growth.

Meaning:

- missing required fields must fail
- invalid required field types must fail
- unknown event types must fail
- unknown top-level extra fields should be ignored, not rejected, so long as
  the known bounded seam remains intact

That keeps the sample honest without making it too brittle against upstream
result evolution.

## 9. Important field boundaries

#### `outcome`

This field is required in the frozen sample shape.

It should stay small and bounded:

- `completed`
- `failed`

Rules:

- `completed` must not carry `error_label`
- `failed` must carry one short `error_label`

This field belongs to the sample shape, not to a claim that LiveKit exposes one
universal result-status contract for every runtime surface.

#### `events`

This field is required and is the actual center of the seam.

It must remain:

- ordered
- typed
- bounded
- reviewable

Not allowed in v1:

- empty event lists
- unknown event types
- transcript dumps
- audio payloads
- screenshots
- room-state exports
- trace bundles

#### `final_output_ref`

This field is optional in v1.

It must stay:

- absent by default
- a small bounded reference if present
- secondary to the event list

It must not become:

- the primary seam
- a large final payload export
- a hidden transcript dump

#### `error_label`

This field is optional at the top level, but only for failed artifacts.

It should stay:

- short
- classifier-like
- small enough to remain reviewable

It must not become:

- a stack trace
- a long operator narrative
- a transcript excerpt

#### `task_label`

This field is required to keep the sample reviewable without dragging in a full
prompt or transcript.

It should stay:

- short
- descriptive
- bounded to one task label

Not allowed in v1:

- prompt dumps
- full chat history
- full system instructions

## 10. Event shape boundaries

#### `message`

Required fields:

- `type`
- `role`
- `content`

Rules:

- `content` must be a short string only
- no content arrays
- no transcript chunks
- no multi-turn conversation dumps
- no speech/audio payloads

#### `function_call`

Required fields:

- `type`
- `name`

Optional field:

- `arguments_ref`

Rules:

- keep any arguments reference bounded
- do not include raw full argument blobs in v1

#### `function_call_output`

Required fields:

- `type`
- `name`

Optional field:

- `status`

Rules:

- keep `status` short if present
- do not include raw tool output bodies
- do not include error transcript blobs

#### `agent_handoff`

Required fields:

- `type`
- `new_agent`

Rules:

- keep `new_agent` to a bounded label/reference only
- do not treat handoff as delegation-success truth
- do not import broader route provenance or trust semantics

## 11. Assay-side meaning

The sample may only claim bounded testing-result observation.

Assay must not treat as truth:

- production runtime correctness
- session correctness
- room correctness
- audio correctness
- handoff correctness
- tool correctness
- user satisfaction or task success beyond the observed artifact

Common anti-overclaim sentence:

> We are not asking Assay to inherit LiveKit session semantics, room
> observability semantics, transcript semantics, or runtime correctness
> semantics as truth.

## 12. Concrete repo deliverable

If this plan is accepted, the next implementation PR should add:

- `examples/livekit-runresult-evidence/README.md`
- `examples/livekit-runresult-evidence/requirements.txt` only if a tiny local
  helper truly needs it
- `examples/livekit-runresult-evidence/generate_synthetic_result.py` only if a
  clean generator remains tiny and deterministic
- `examples/livekit-runresult-evidence/map_to_assay.py`
- `examples/livekit-runresult-evidence/fixtures/valid.livekit.json`
- `examples/livekit-runresult-evidence/fixtures/failure.livekit.json`
- `examples/livekit-runresult-evidence/fixtures/malformed.livekit.json`
- `examples/livekit-runresult-evidence/fixtures/valid.assay.ndjson`
- `examples/livekit-runresult-evidence/fixtures/failure.assay.ndjson`

Fixture boundary notes:

- v1 fixtures may omit every optional top-level field
- v1 fixtures should keep the shape obviously testing-result-first
- v1 fixtures must not include transcript dumps or audio payloads
- v1 fixtures should preferably include one `agent_handoff` event in the valid
  case so the lane stays visibly distinct from Browser Use

## 13. Generator policy

The implementation should prefer a real local generator **only if** it stays
small and deterministic.

### 13.1 Preferred path

Preferred:

- docs-backed frozen artifacts
- a tiny mapper that validates the frozen shape
- no room setup
- no cloud dependency
- no credentials
- no audio pipeline exercise

### 13.2 Hard fallback rule

If a real local generator would require:

- a LiveKit server
- realtime room orchestration
- provider credentials
- audio hardware or speech pipeline setup
- a full runtime tutorial heavy enough to overshadow the seam

then the sample should stay on a **docs-backed frozen artifact shape**.

That fallback is especially appropriate here because the goal is to isolate the
smallest honest testing-result seam, not to recreate a full voice-agent stack
inside this repo.

## 14. Valid, failure, malformed corpus

The first sample should follow the established corpus pattern.

### 14.1 Valid

One successful testing artifact with:

- `outcome=completed`
- one `message`
- one `function_call`
- one `function_call_output`
- preferably one `agent_handoff`
- no `error_label`

### 14.2 Failure

One failed testing artifact with:

- `outcome=failed`
- a small event list
- one short `error_label`
- no transcript-like bodies

### 14.3 Malformed

One malformed artifact that fails fast, for example:

- missing `events`
- unsupported event type
- `completed` with `error_label`
- `failed` without `error_label`
- malformed `message.content` shape

## 15. Outward strategy

Do not open an outward GitHub issue for LiveKit in the first step.

The better first channel is:

- LiveKit Community
- `Agents` category
- short technical question

Why:

- the repo has no Discussions
- blank issues are disabled
- issue templates are structured around bug reports and feature requests
- the repo explicitly points technical discussion toward the community

Suggested outward title:

> Question: is `voice.testing.RunResult` the right small external-consumer
> seam?

Suggested outward question:

> If an external evidence consumer wants the smallest honest LiveKit Agents
> surface, is a bounded artifact derived from `voice.testing.RunResult.events`
> roughly the right place to start, with final output treated as optional bonus
> context only, or is there an even thinner testing-result surface you would
> rather point them at?

## 16. GitHub escalation rule

Only open a GitHub issue if one of these becomes true:

- community feedback says a capability is missing and should be requested
- the sample exposes a concrete seam gap in the SDK
- maintainers point to GitHub as the better route for the specific ask

If escalation becomes necessary:

- use the feature request template
- frame it as a concrete missing testing/result capability
- do not frame it as an open-ended research question

## 17. Sequencing rule

This lane should stay inside the same one-lane-at-a-time discipline.

Meaning:

1. formalize `P16` now
2. build the `P16` sample on `main`
3. let the freshest active outward lanes breathe
4. keep near-term follow-up attention on the warmer current lanes like
   Pydantic AI and Langfuse
5. open the LiveKit community question only after the sample is live

## 18. Non-goals

This plan does not:

- define a LiveKit telemetry export contract
- define a room metrics export contract
- define a transcript export lane
- define a raw audio export lane
- define session correctness as Assay truth
- define runtime success as Assay truth

## References

- [LiveKit Agents — Testing and evaluation](https://docs.livekit.io/agents/start/testing/)
- [LiveKit Agents JS — `voice.testing`](https://docs.livekit.io/reference/agents-js/modules/agents.voice.testing.html)
- [LiveKit Agents JS — `RunResult`](https://docs.livekit.io/reference/agents-js/classes/agents.voice.testing.RunResult.html)
- [LiveKit Agents Python — `voice.testing`](https://docs.livekit.io/reference/python/v1/livekit/agents/voice/testing.html)
- [LiveKit Agents repo](https://github.com/livekit/agents)
- [LiveKit Community](https://community.livekit.io/)
