# PLAN — P12 Browser Use History / Output Evidence Interop (2026 Q2)

- **Date:** 2026-04-08
- **Owner:** Evidence / Product
- **Status:** Planning
- **Scope (this PR):** Define the active Browser Use adjacent-space lane after
  Agno. This remains separate from the planned `P11` commerce / trust-proof
  family. No sample implementation, no outward Discussion, no contract freeze
  in this slice.

## 1. Why this plan exists

After the current wave, the next adjacent-space lane should still pass the same
three tests:

1. the upstream project already exposes one bounded surface,
2. Assay can consume that surface without inheriting upstream semantics as
   truth,
3. the repo has a natural maintainer channel for one small sample-backed
   question.

`browser-use/browser-use` currently fits that pattern well:

- the repo is large, active, and visibly growing
- GitHub Discussions are enabled
- the docs expose a clear local output/history surface through
  `AgentHistoryList`, `action_history()`, `final_result()`, `errors()`, and
  `structured_output`
- that surface is different enough from the trace-first seams already tested in
  other lanes

At the same time, Browser Use also documents Laminar, OpenLIT, and telemetry.
That broader observability layer is real, but it is not the right first seam
for Assay.

This is **not** a telemetry plan.

This is **not** a Laminar export plan.

This is **not** an OpenLIT export plan.

This is a plan for a **bounded run-history / output seam derived from the
documented Browser Use result surface**.

## 2. Position in the broader queue

The broader post-Agno ranking now treats the `P11` commerce / trust-proof lane
as strategically higher than Browser Use:

- `P11A` — Visa TAP
- `P11B` — x402
- `P11C` — Identus watchlist

That ranking is about long-range priority, not about leaving half-finished
adjacent work behind.

Execution rule for now:

- finish the Browser Use planning slice cleanly
- keep it separate from the `P11` commerce family
- only then decide whether the next new lane after Browser Use is `P11A` or
  `Langfuse`

## 3. Hard positioning rule

This lane must not overclaim what the sample actually observes.

Normative framing:

> This sample targets the smallest honest local run-result surface exposed by
> Browser Use, not an observability export, cloud trace, telemetry stream, or
> MCP protocol record.

That means:

- Browser Use is the upstream context, not the truth source
- `AgentHistoryList` and its small accessors are run-result surfaces, not
  policy or observability truth surfaces
- Assay stays an external evidence consumer, not a judge of browser automation
  correctness, page correctness, or cloud telemetry correctness

## 4. Why not observability-first

The repo and docs make it tempting to start with observability because Browser
Use documents:

- Laminar integration
- OpenLIT integration
- telemetry

That would be the wrong first wedge.

Why:

- it would look too similar to earlier trace and telemetry lanes
- it would turn an adjacent-space opportunity into another generic
  observability pitch
- it would ignore the smaller official surface already documented in the
  output-format docs

The cleaner first wedge is:

- one artifact derived from the documented `AgentHistoryList` path
- a bounded `action_history` reduction derived from documented selectors such
  as `action_names()` and, only if needed, a small reducer over the recorded
  steps
- a short `final_result` representation derived from `final_result()`
- bounded error observations derived from `errors()`
- optional structured output only if the chosen sample shape naturally carries
  it

This keeps the Browser Use lane clearly different from MAF, OpenAI Agents,
LangGraph, and the observability-heavy platform candidates.

## 5. Recommended v1 seam

Use **one frozen serialized artifact derived from the documented
`AgentHistoryList` result surface** as the first external-consumer seam.

The primary documented selectors are:

- `action_names()`
- `final_result()`
- `errors()`

`structured_output` is documented too, but it should remain secondary in v1
because it quickly turns the seam into application-schema territory.

The frozen sample shape may still use fields named `action_history`,
`final_result`, and `errors`, but those names should be treated as Assay-side
sample reductions derived from documented selectors, not as claims that
Browser Use already guarantees those exact serialized export fields.

This seam is:

- output-first
- history-first
- local
- reviewable
- smaller than telemetry
- smaller than Laminar or OpenLIT export
- meaningfully different from trace-first interop

Important framing rule:

> The sample uses a frozen serialized artifact derived from the documented
> `AgentHistoryList` result surface, not a claim that Browser Use already
> guarantees a fixed wire-export contract.

## 6. v1 artifact contract

### 6.1 Required fields

The first sample should require:

- `schema`
- `framework`
- `surface`
- `task_label`
- `timestamp`
- `outcome`
- `action_history`
- `final_result`
- `errors`

These required fields belong to the frozen sample artifact shape.

They must be described as:

- sample-level reductions derived from documented Browser Use result selectors
- not an upstream guarantee that Browser Use ships one canonical serialized
  export object with these exact field names

### 6.2 Optional fields

The first sample may include:

- `structured_output_ref`
- `history_summary`
- `browser_ref`
- `url_ref`

### 6.3 Important field boundaries

#### `action_history`

This field is required in the frozen sample shape.

It should be described as a **sample-level bounded reduction** derived from the
documented `AgentHistoryList` surface, preferably anchored in selectors such as
`action_names()` and only widened by a small reducer if the chosen sample shape
needs one.

It should stay small and bounded:

- simple action names or action labels
- optional short status labels only if the chosen sample shape carries them
- short target references only if already present

Not allowed in v1:

- full page DOM dumps
- screenshots
- raw page HTML
- network payloads
- cloud-only telemetry annotations

This requirement belongs to the sample shape, not to an upstream claim that
Browser Use guarantees one universal serialized `action_history` contract.

#### `final_result`

This field is required in the frozen sample shape, but it should be framed as a
**short frozen representation** derived from `final_result()`, not necessarily
the full upstream extracted-content body.

It remains upstream output semantics only.

It must not be promoted into:

- task-success truth beyond the observed run artifact
- policy truth
- browser-state truth

The sample should prefer a short final result string or short bounded object,
not a large application payload or a claim that Browser Use exports one fixed
serialized `final_result` wire field.

#### `errors`

This field is required in the frozen sample shape, even for successful runs.

It should be framed as **bounded error observations** derived from `errors()`,
not as a claim that Browser Use guarantees one universal small serialized error
contract.

v1 should keep it simple:

- empty list for a clean run
- one or more short error labels for a failed run

The sample must not treat Browser Use error labels as normative correctness or
safety judgments.

#### `structured_output_ref`

This field is optional in v1.

The sample should prefer omitting it unless it is naturally present in the
chosen export shape.

If present, it must remain a small bounded reference or label and must not
smuggle in a full application-specific schema as a second seam.

#### References

The optional reference fields must stay bounded:

- small label
- opaque id
- short reference string

Not allowed in v1:

- full browser session state
- full URL crawl graph
- screenshots or visual payloads
- complete structured output schema bodies

## 7. Assay-side meaning

The sample may only claim bounded run-result observation.

Assay must not treat as truth:

- browser automation correctness
- workflow correctness
- page correctness
- user intent correctness
- Laminar or OpenLIT semantics
- telemetry semantics

Common anti-overclaim sentence:

> We are not asking Assay to inherit Browser Use action history, output
> semantics, browser-state semantics, or observability semantics as truth.

## 8. Concrete repo deliverable

If this plan is accepted, the next implementation PR should add:

- `examples/browser-use-history-evidence/README.md`
- `examples/browser-use-history-evidence/requirements.txt` only if a clean
  local generator truly needs it
- `examples/browser-use-history-evidence/generate_synthetic_result.py` only if
  a small local generator is viable
- `examples/browser-use-history-evidence/map_to_assay.py`
- `examples/browser-use-history-evidence/fixtures/valid.browser-use.json`
- `examples/browser-use-history-evidence/fixtures/failure.browser-use.json`
- `examples/browser-use-history-evidence/fixtures/malformed.browser-use.json`
- `examples/browser-use-history-evidence/fixtures/valid.assay.ndjson`
- `examples/browser-use-history-evidence/fixtures/failure.assay.ndjson`

Fixture boundary notes:

- v1 fixtures may omit every optional reference field
- v1 fixtures must not include screenshots or page dumps
- v1 fixtures should keep the export shape obviously history-first rather than
  telemetry-first

## 9. Generator policy

The implementation should prefer a real local generator **only if** it stays
small and deterministic.

### 9.1 Preferred path

Preferred:

- a local generator that exercises the documented result surface directly
- no cloud dependency
- no Laminar dependency
- no OpenLIT dependency
- no telemetry setup heavy enough to overshadow the sample
- no browser automation setup heavy enough to turn the sample into an
  environment tutorial

### 9.2 Hard fallback rule

If a real local generator would require:

- brittle browser runtime setup
- heavyweight system dependencies
- unstable page automation
- credentials
- cloud or telemetry configuration

then the sample should fall back to a **docs-backed frozen artifact shape**.

That fallback is acceptable here because the plan is about the smallest honest
external-consumer seam, not about proving a complete Browser Use runtime stack
inside this repo.

## 10. Valid, failure, malformed corpus

The first sample should follow the established corpus pattern.

### 10.1 Valid

One successful run with:

- small `action_history`
- short `final_result`
- empty `errors`

### 10.2 Failure

One failed run with:

- small `action_history`
- either an empty or bounded `final_result`
- one short error entry

### 10.3 Malformed

One malformed artifact that fails fast, for example:

- missing `action_history`
- missing `final_result`
- `errors` not a list

## 11. Outward strategy

Do not open a Browser Use Discussion until the sample is on `main`.

After that:

- one small GitHub Discussion
- one link
- one boundary question
- no broad product pitch
- no observability pitch

Preferred channel shape:

- the most question-oriented Discussion category available at posting time

Suggested outward question:

> If an external evidence consumer wants the smallest honest Browser Use
> surface, is an artifact derived from `AgentHistoryList` and the documented
> `action_history()` / `final_result()` / `errors()` path roughly the right
> place to start, or is there a thinner result surface you would rather point
> them at?

## 12. Sequencing rule

This lane should still respect the one-lane-at-a-time discipline.

Meaning:

1. plan first
2. sample second
3. outward Browser Use question only after the sample lands
4. let the lane breathe before opening Langfuse
5. decide separately whether `P11A` or Langfuse comes next after Browser Use

## 13. Non-goals

This plan does not:

- define a Laminar or OpenLIT adapter
- define a telemetry-first interop lane
- define a Browser Use Cloud export contract
- define an MCP server evidence contract
- define browser automation correctness as Assay truth

## References

- [TODO — Next Upstream Interop Lanes (2026 Q2)](./TODO-NEXT-UPSTREAM-INTEROP-LANES-2026q2.md)
- [PLAN — P10 Agno Accuracy Eval Evidence Interop](./PLAN-P10-AGNO-ACCURACY-EVAL-EVIDENCE-2026q2.md)
- [ADR-033: OTel Trust Compiler Positioning](./ADR-033-OTel-Trust-Compiler-Positioning.md)
