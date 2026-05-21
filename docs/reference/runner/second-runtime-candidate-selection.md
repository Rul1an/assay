# Assay-Runner Second Runtime Candidate Selection

> Internal Phase 2B selection note. This page records evaluations of concrete
> runtime candidates against the entry plan in
> [`second-runtime-plan.md`](second-runtime-plan.md). It is not a runtime
> selection record yet, not a dependency proposal, and not a fixture design.

**Status:** skeleton. This note does not select a candidate yet. Each
candidate section below is a placeholder. The evaluation form is fixed; the
content is added one candidate per iteration.

This page lays down the evaluation form for the deliverable defined by
<https://github.com/Rul1an/assay/issues/1295>. The deliverable's acceptance
criteria (candidate evaluations and binary outcomes) are not yet met by
this skeleton; per-candidate PRs land under `## Candidates` to complete
the deliverable iteratively.

## Evaluation Discipline

Each candidate is evaluated independently against the seven Candidate
Requirements from
[`second-runtime-plan.md` § Candidate Requirements](second-runtime-plan.md#candidate-requirements).
Evaluations are conservative: when public evidence is thin, the outcome is
`insufficient evidence`, not an optimistic `qualifies`.

A candidate is not selected by this note simply by appearing here. Selection
requires `qualifies` against all seven requirements **and** an explicit
selection statement in the Selection Outcome section below.

## Outcome Vocabulary

Every requirement evaluation uses exactly one of:

| Outcome | Meaning |
|---|---|
| `qualifies` | Public evidence clearly shows the requirement is met. The evaluation row cites the specific evidence. |
| `does not qualify` | Public evidence shows the requirement is not met. The line is closed for this candidate. |
| `insufficient evidence` | The requirement might be met, but current public evidence is not strong enough to say so. The candidate is paused, not rejected. |

`insufficient evidence` is a first-class outcome. It must not be promoted to
`qualifies` because a reviewer feels the gap is small.

A candidate's overall outcome equals the **lowest** outcome across its seven
requirement rows:

- all seven `qualifies` → candidate overall `qualifies`
- any `does not qualify` → candidate overall `does not qualify`
- otherwise (any `insufficient evidence`, no `does not qualify`) → candidate
  overall `insufficient evidence`

## Stable Identity — Level-3 Checklist

The Stable identity requirement uses the level-3 interpretation defined in
[`#1295` Stable identity — required interpretation](https://github.com/Rul1an/assay/issues/1295).
A candidate satisfies Stable identity only if at least one field meets **all
three** conditions:

| Condition | Definition | Acceptable evidence |
|---|---|---|
| **runtime-generated** | The value is produced by the runtime/SDK, not by the fixture or adapter | Public docs or typed API contract showing runtime generation |
| **binding-intended** | The field is meant to bind the same tool call or action across event boundaries | Public docs describing the field's binding purpose, or a stable SDK event schema explicitly modeling it |
| **run-window unique** | The value is unique within the run window for one action | Public docs or schema confirming uniqueness scope |

Source code may support the evaluation, but a `qualifies` outcome should
prefer public docs, typed API contracts, or stable SDK event schemas over
incidental implementation details. An internal variable name found by source
archaeology does not establish binding-intent on its own.

Disqualifying identity signals (these never satisfy Stable identity):

- generic trace ids without tool-call binding semantics
- request, response, or message ids without binding-intent in the runtime
  contract
- fixture-injected ids (the fixture chose the value, not the runtime)
- ids that exist only by source archaeology, not in the public contract
- ids that are not stable across the same tool call's event boundaries

## Candidate Evaluation Form

Each candidate is recorded as one subsection under `## Candidates` below. The
subsection follows this fixed form. Reviewers should be able to read the
candidate row-by-row and reach the same overall outcome.

```markdown
### Candidate: <runtime name>

| # | Requirement | Outcome | Evidence |
|---|---|---|---|
| 1 | Offline execution | qualifies / does not qualify / insufficient evidence | <link or quote> |
| 2 | Stable identity | qualifies / does not qualify / insufficient evidence | <see Stable Identity row below> |
| 3 | Comparable surface | qualifies / does not qualify / insufficient evidence | <link or quote> |
| 4 | Deterministic dependency lock | qualifies / does not qualify / insufficient evidence | <link or quote> |
| 5 | Linux/eBPF fit | qualifies / does not qualify / insufficient evidence | <link or quote> |
| 6 | Small event shape | qualifies / does not qualify / insufficient evidence | <link or quote> |
| 7 | Evidence boundary fit | qualifies / does not qualify / insufficient evidence | <link or quote> |

**Stable identity detail (only filled if row 2 is `qualifies`):**

- Field name: `<field>`
- Source: `<event type / API entrypoint>`
- runtime-generated evidence: <link or quote>
- binding-intended evidence: <link or quote>
- run-window unique evidence: <link or quote>

**Expected delegated gate for first fixture PR (only filled if overall outcome is `qualifies`):**
`gates=all` per [`second-runtime-plan.md` § Suggested PR Sequence](second-runtime-plan.md#suggested-pr-sequence)
step 4. A narrower gate is later coordinated work; do not introduce it as a
side effect of the first fixture PR.

**Overall outcome:** `qualifies` / `does not qualify` / `insufficient evidence`

**Notes:** <optional, one or two sentences of context — no advocacy language>
```

## Candidates

### Candidate: Anthropic SDK direct

The Anthropic SDK direct path means using
[`anthropic-sdk-python`](https://github.com/anthropics/anthropic-sdk-python)
or `@anthropic-ai/sdk` against the Messages API with a custom client tool, not
through `@openai/agents-js` and not through `Anthropic Agent SDK` higher-level
wrappers. This isolates the smallest possible second-runtime surface.

| # | Requirement | Outcome | Evidence |
|---|---|---|---|
| 1 | Offline execution | qualifies | Cassette-replay via [VCR.py](https://vcrpy.readthedocs.io/en/latest/usage.html) (Python) or `nock`/MSW (TypeScript) works at the HTTPS layer against `api.anthropic.com`. Live API key is required only during one-time cassette recording (curation step), never during delegated acceptance. `record_mode='none'` guarantees no network calls during fixture execution. Restrictions: non-streaming `client.messages.create()` only (streaming cassettes complicate determinism); `anthropic-version` header pinned; re-recording is maintainer-controlled, analogous to the [`@openai/agents` bump flow in `fixtures-v0.md`](fixtures-v0.md#dependency-upgrade-contract). |
| 2 | Stable identity | insufficient evidence | Two of three level-3 conditions pass; one is implicit only. See Stable identity detail below. |
| 3 | Comparable surface | not yet evaluated | Row 2 blocks overall outcome; remaining rows recorded as `not yet evaluated` to avoid speculative claims. Re-evaluate when row 2 is unblocked. |
| 4 | Deterministic dependency lock | not yet evaluated | Row 2 blocks overall outcome. |
| 5 | Linux/eBPF fit | not yet evaluated | Row 2 blocks overall outcome. |
| 6 | Small event shape | not yet evaluated | Row 2 blocks overall outcome. |
| 7 | Evidence boundary fit | not yet evaluated | Row 2 blocks overall outcome. |

**Stable identity detail (row 2):**

The candidate identity field is `tool_use.id` in the
[Messages API tool_use content block](https://platform.claude.com/docs/en/agents-and-tools/tool-use/handle-tool-calls).
Example value from public docs: `"toolu_01A09q90qw90lq917835lq9"`.

- Field name: `tool_use.id`
- Source: `tool_use` content block in the assistant message of a Messages API response
- **runtime-generated evidence:** `insufficient evidence`. Public docs describe the `id` as a field on the `tool_use` block contained in the API response, and the SDK has no documented client-side generation path. The example value's `toolu_` prefix matches Anthropic's other server-issued identifier conventions (`msg_`, etc.). However, no public docs sentence explicitly states that the Anthropic API server generates this field. Per the level-3 strict reading codified in the [Stable Identity Checklist](#stable-identity--level-3-checklist), implicit chain-of-reasoning evidence does not satisfy `runtime-generated`. A `qualifies` outcome requires either an explicit docs statement or a typed SDK schema annotation that names the field as server-generated.
- **binding-intended evidence:** `qualifies`. The [handle-tool-calls docs](https://platform.claude.com/docs/en/agents-and-tools/tool-use/handle-tool-calls) state literally: *"`id`: A unique identifier for this particular tool use block. This will be used to match up the tool results later."* The same docs define `tool_result.tool_use_id` as *"The `id` of the tool use request this is a result for."* A 400-error message documented in the same page (*"tool_use ids were found without tool_result blocks immediately after"*) confirms that the API enforces the binding contract at request validation time.
- **run-window unique evidence:** `qualifies`. The [handle-tool-calls docs](https://platform.claude.com/docs/en/agents-and-tools/tool-use/handle-tool-calls) state literally: *"A unique identifier for this particular tool use block."*

**Expected delegated gate for first fixture PR (only filled if overall outcome is `qualifies`):**
Not applicable. Overall outcome below is `insufficient evidence`, so no first fixture PR may be opened from this evaluation.

**Overall outcome:** `insufficient evidence`

Per the lowest-row-wins rule in [Evaluation Discipline](#evaluation-discipline),
row 2's `insufficient evidence` determines the candidate's overall outcome.
Rows 3-7 remain `not yet evaluated` because evaluating them would not change
the overall outcome and would invite optimistic interpretation. They become
relevant only if `runtime-generated` evidence improves.

**Notes:** Anthropic SDK direct is technically promising on rows 1, 2.b, 2.c.
The single blocker is the absence of one explicit docs sentence or typed schema
annotation that names `tool_use.id` as server-generated. This evaluation is
recorded so the same research does not have to be repeated when the docs or
SDK schema improve.

### Candidate: PydanticAI

The PydanticAI path means using [PydanticAI](https://ai.pydantic.dev/) as the
runtime layer, with its built-in offline test helpers `TestModel` and
`FunctionModel`, exercising a custom `read_file`-style tool. PydanticAI is an
agent-framework that sits between the developer and an underlying LLM
provider, which affects the identity-source analysis below.

| # | Requirement | Outcome | Evidence |
|---|---|---|---|
| 1 | Offline execution | qualifies | PydanticAI provides built-in offline helpers `TestModel` and `FunctionModel` ([Testing docs](https://ai.pydantic.dev/testing/)). `TestModel` is described as *"plain old procedural Python code"* with no model calls and no API key. `FunctionModel` allows developer-controlled deterministic tool-call generation. No cassette infrastructure is required. Restrictions: PydanticAI version pin via `pyproject.toml`/lockfile; `TestModel` and `FunctionModel` schemas may evolve across framework versions, so re-validation on bumps is maintainer-controlled. |
| 2 | Stable identity | does not qualify | PydanticAI documents that the framework itself generates `tool_call_id` values when the underlying model does not supply one. This is anti-evidence, not absent evidence. See Stable identity detail below. |
| 3 | Comparable surface | not evaluated after disqualifying stable-identity failure | Row 2 disqualifies the candidate. Remaining rows are not evaluated to avoid implying that "fixing" row 2 would be possible without changing PydanticAI's documented design. |
| 4 | Deterministic dependency lock | not evaluated after disqualifying stable-identity failure | Same as row 3. |
| 5 | Linux/eBPF fit | not evaluated after disqualifying stable-identity failure | Same as row 3. |
| 6 | Small event shape | not evaluated after disqualifying stable-identity failure | Same as row 3. |
| 7 | Evidence boundary fit | not evaluated after disqualifying stable-identity failure | Same as row 3. |

**Stable identity detail (row 2):**

The candidate identity field is `ToolCallPart.tool_call_id` in
[`pydantic_ai.messages`](https://pydantic.dev/docs/ai/api/pydantic-ai/messages/).

- Field name: `ToolCallPart.tool_call_id`
- Source: `ToolCallPart` content in PydanticAI message parts
- **runtime-generated evidence:** `does not qualify`. Public PydanticAI docs state literally: *"The tool call identifier, this is used by some models including OpenAI. **In case the tool call id is not provided by the model, Pydantic AI will generate a random one.**"* The field is declared with `field(default_factory=_generate_tool_call_id)`. This is documented adapter-side ID generation: the framework itself can produce the value when the underlying provider does not. Per the level-3 strict reading codified in the [Stable Identity Checklist](#stable-identity--level-3-checklist), the value must be generated by the runtime/SDK, **not chosen by the fixture or adapter**. PydanticAI's framework layer satisfies the definition of "adapter" here. Additionally, `TestModel` and `FunctionModel` offline strategies produce tool calls without any provider, so the identity source is necessarily framework- or fixture-generated in those configurations. This is anti-evidence in the public docs, not a documentation gap; no future docs improvement will change the behavior because the behavior is the documented design.
- **binding-intended evidence:** `qualifies`. PydanticAI uses `tool_call_id` to bind `ToolCallPart` to `ToolReturnPart`, which is the framework's documented mechanism for matching tool execution results to original requests. This sub-condition is met; it is overridden by the runtime-generated failure above.
- **run-window unique evidence:** `qualifies`. The default factory `_generate_tool_call_id` produces random identifiers intended to be unique within a run. This sub-condition is met; it is overridden by the runtime-generated failure above.

**Expected delegated gate for first fixture PR (only filled if overall outcome is `qualifies`):**
Not applicable. Overall outcome below is `does not qualify`.

**Overall outcome:** `does not qualify`

Per the lowest-row-wins rule in [Evaluation Discipline](#evaluation-discipline),
row 2's `does not qualify` determines the candidate's overall outcome.
Other rows are not evaluated; re-evaluating them would not change the overall
outcome and would suggest that PydanticAI could be "fixed" to qualify, which
would require a redesign of its tool-call identity model.

**Notes:** This result suggests a review warning for agent-framework
candidates: if the framework can synthesize tool-call IDs when the underlying
provider does not supply one, it cannot satisfy the level-3 stable identity
rule as an independent runtime without a stricter provider-backed identity
contract. This is a *pattern observation* from one evaluation, not a blanket
rejection of agent frameworks; LangChain, LlamaIndex, AutoGen, and CrewAI must
each be evaluated against their own public docs in their own PR. Adapter-side
ID fallback is a disqualifying pattern under the level-3 rule. Frameworks that
expose a strict provider-backed mode (no fallback, propagate-or-fail) may
qualify on row 2 even if their default mode does not.

## Selection Outcome

**No candidate currently qualifies.**

Evaluated candidates and their overall outcomes:

| Candidate | Overall outcome |
|---|---|
| Anthropic SDK direct | `insufficient evidence` |
| PydanticAI | `does not qualify` |

The selection issue
[`#1295`](https://github.com/Rul1an/assay/issues/1295) remains open until a
candidate evaluation reaches `qualifies` AND this section is explicitly
updated to name that candidate as the selected second runtime.

Re-evaluation of an `insufficient evidence` outcome happens in a separate PR
that cites the new public evidence which closes the previous gap; it does
not happen by reinterpreting the same evidence more optimistically.

## Non-Goals For This Note

This selection note does not:

- propose runtime dependencies
- propose fixture code
- introduce call-id-less correlation fallback
- propose a narrower delegated gate for the second runtime
- propose cross-runtime capability-diff against S5
- promote level-1 or level-2 identity signals to satisfy Stable identity
- promote an `insufficient evidence` outcome to `qualifies`

Each of those is a separate decision that follows only after this note
records a clear `qualifies` outcome.

## How To Add A Candidate

1. Open a docs-only PR titled along the lines of
   `[codex] evaluate <runtime> against second runtime entry plan`.
2. Add one subsection under `## Candidates` using the form above.
3. Cite public docs, typed API contracts, or stable SDK event schemas in the
   Evidence cells. Source code citations are acceptable as supporting
   evidence but not as the sole basis for a `qualifies` row, especially for
   row 2 (Stable identity).
4. Do not alter the Evaluation Discipline, Outcome Vocabulary, Stable
   Identity checklist, or Candidate Evaluation Form sections except through a
   separate contract-discipline PR.
5. Set the candidate's overall outcome by the lowest row outcome rule above.
6. Only when an overall outcome is `qualifies` AND the Selection Outcome
   section is updated to name this candidate as the selected second runtime,
   may the first fixture implementation PR be opened.

## References

- [Runner second runtime Phase 2B plan](second-runtime-plan.md)
- [Runner CI lane contract](ci-lanes.md)
- [Runner acceptance fixture v0 contract](fixtures-v0.md)
- [Runner capability-diff v0 contract](capability-diff-v0.md)
- [Assay-Runner boundary and extraction map](boundary-map.md)
- Selection issue: <https://github.com/Rul1an/assay/issues/1295>
