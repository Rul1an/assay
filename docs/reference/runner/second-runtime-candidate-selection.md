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

### Candidate: OpenAI Python SDK direct

The OpenAI Python SDK direct path means using
[`openai-python`](https://github.com/openai/openai-python) against the Chat
Completions API with a custom client tool, not through `@openai/agents-js` and
not through Assistants/Responses higher-level surfaces. This isolates the
smallest possible second-runtime surface using OpenAI as the underlying
provider, paralleling the Anthropic SDK direct evaluation above.

| # | Requirement | Outcome | Evidence |
|---|---|---|---|
| 1 | Offline execution | qualifies | Cassette-replay via [VCR.py](https://vcrpy.readthedocs.io/en/latest/usage.html) (Python) at the HTTPS layer against `api.openai.com`. Live API key is required only during one-time cassette recording (curation step), never during delegated acceptance. `record_mode='none'` guarantees no network calls during fixture execution. Restrictions: non-streaming `client.chat.completions.create()` only (streaming cassettes complicate determinism); `openai-python` SDK version pinned via `requirements.txt`/lockfile; cassette request/response shape stable across compatible SDK patch versions; re-recording is maintainer-controlled, analogous to the [`@openai/agents` bump flow in `fixtures-v0.md`](fixtures-v0.md#dependency-upgrade-contract). |
| 2 | Stable identity | insufficient evidence | One of three level-3 sub-conditions passes via public docs; two are gaps. See Stable identity detail below. |
| 3 | Comparable surface | not yet evaluated | Row 2 blocks overall outcome; remaining rows recorded as `not yet evaluated` to avoid speculative claims. Re-evaluate when row 2 is unblocked. |
| 4 | Deterministic dependency lock | not yet evaluated | Row 2 blocks overall outcome. |
| 5 | Linux/eBPF fit | not yet evaluated | Row 2 blocks overall outcome. |
| 6 | Small event shape | not yet evaluated | Row 2 blocks overall outcome. |
| 7 | Evidence boundary fit | not yet evaluated | Row 2 blocks overall outcome. |

**Stable identity detail (row 2):**

The candidate identity field is `tool_calls[].id` in
[`ChatCompletionMessageFunctionToolCall`](https://github.com/openai/openai-python/blob/e75766769547601a25ed83b666c4d0fd046881f0/src/openai/types/chat/chat_completion_message_function_tool_call.py).
Example value reported in public sources: `"call_DdmO9pD3xa9XTPNJ32zg2hcA"`.

- Field name: `ChatCompletionMessageFunctionToolCall.id`
- Source: `tool_calls` array on the assistant message of a Chat Completions API response
- **runtime-generated evidence:** `insufficient evidence`. The typed SDK docstring for the field reads literally *"The ID of the tool call."* — four words, with no mention of who generates the value. The example value's `call_` prefix matches OpenAI's other server-issued identifier conventions, and the SDK has no documented client-side generation path. However, no public docs sentence or typed SDK annotation explicitly states that the OpenAI API server generates this field. Per the level-3 strict reading codified in the [Stable Identity Checklist](#stable-identity--level-3-checklist), implicit chain-of-reasoning evidence does not satisfy `runtime-generated`.
- **binding-intended evidence:** `qualifies`. The typed SDK docstring for [`ChatCompletionToolMessageParam.tool_call_id`](https://github.com/openai/openai-python/blob/e75766769547601a25ed83b666c4d0fd046881f0/src/openai/types/chat/chat_completion_tool_message_param.py) reads literally *"Tool call that this message is responding to."* The Function Calling guide demonstrates the binding mechanism in code with `tool_call_id=tool_call.id`, establishing the documented binding contract between the assistant's tool call and the user's tool response message.
- **run-window unique evidence:** `insufficient evidence`. Unlike the Anthropic SDK direct evaluation, the OpenAI typed SDK docstring does **not** contain the word "unique" — it reads only *"The ID of the tool call."* The official Function Calling guide does not provide an explicit uniqueness guarantee for parallel tool calls within a single response; secondary sources describe parallel tool calls each having "a unique id" but this could not be verified verbatim in primary OpenAI documentation. Per the level-3 strict reading, an implicit uniqueness expectation does not satisfy `run-window unique`.

**Expected delegated gate for first fixture PR (only filled if overall outcome is `qualifies`):**
Not applicable. Overall outcome below is `insufficient evidence`, so no first fixture PR may be opened from this evaluation.

**Overall outcome:** `insufficient evidence`

Per the lowest-row-wins rule in [Evaluation Discipline](#evaluation-discipline),
row 2's `insufficient evidence` determines the candidate's overall outcome.
Rows 3-7 remain `not yet evaluated` because evaluating them would not change
the overall outcome and would invite optimistic interpretation. They become
relevant only if both `runtime-generated` and `run-window unique` evidence
improves.

**Notes:** This evaluation found two level-3 identity gaps where the Anthropic
SDK direct evaluation found one. That comparison is observational only: it
reflects the public documentation reviewed here, not a claim about the
underlying provider behavior. No broader direct-SDK pattern claim is made;
two data points are too few to support a pattern observation.

### Candidate: Vercel AI SDK

The Vercel AI SDK path means using
[`ai`](https://github.com/vercel/ai) (the `ai` npm package) as the runtime
layer, with `generateText` or `streamText` exercising a custom `read_file`-style
tool. The SDK is a wrapper-layer between developer code and an underlying LLM
provider. Two offline-execution routes exist, with different level-3 identity
risk profiles. Both are documented below.

The **primary evaluation** treats Vercel AI SDK as an independent runtime,
which requires path A (Vercel's own `MockLanguageModelV2` offline helper). The
**alternate evaluation** treats Vercel AI SDK as a provider-wrapper using
cassette + a real provider, which is recorded for completeness but is not a
selection path because the identity-bearing runtime then is the underlying
provider, not Vercel.

#### Path A — Vercel as independent runtime (MockLanguageModelV2)

| # | Requirement | Outcome | Evidence |
|---|---|---|---|
| 1 | Offline execution | qualifies | Vercel AI SDK provides built-in offline helpers `MockLanguageModelV2`, `MockEmbeddingModelV1`, and `simulateReadableStream` from `ai/test` ([Testing docs](https://sdk.vercel.ai/docs/ai-sdk-core/testing)). No API key required; no network calls. Restrictions: SDK version pin via `package.json`/lockfile; maintainer-controlled re-validation on bumps. |
| 2 | Stable identity | does not qualify | `MockLanguageModelV2` and other developer-supplied mock providers produce tool-call IDs without any underlying provider. The identity source is necessarily mock-runtime- or fixture-generated. See Stable identity detail below. |
| 3 | Comparable surface | not evaluated after disqualifying stable-identity failure | Row 2 disqualifies the candidate on this path. |
| 4 | Deterministic dependency lock | not evaluated after disqualifying stable-identity failure | Same as row 3. |
| 5 | Linux/eBPF fit | not evaluated after disqualifying stable-identity failure | Same as row 3. |
| 6 | Small event shape | not evaluated after disqualifying stable-identity failure | Same as row 3. |
| 7 | Evidence boundary fit | not evaluated after disqualifying stable-identity failure | Same as row 3. |

**Stable identity detail (path A, row 2):**

- Field name: `ToolCallPart.toolCallId` in the Vercel AI SDK message types
- Source on path A: mock provider produced by `MockLanguageModelV2` or developer-supplied mock function
- **runtime-generated evidence:** `does not qualify`. With no underlying provider, the mock setup is the runtime. Whether the mock library auto-generates or the developer supplies an ID, the value is generated by code that sits on the fixture/adapter side of the level-3 boundary. Per the Stable Identity Checklist, the value must be generated by the runtime/SDK, **not chosen by the fixture or adapter**. The path A configuration cannot satisfy this even in principle.
- **binding-intended evidence:** `qualifies` (overridden by the runtime-generated failure above). Vercel AI SDK documents `toolCallId` as the binding mechanism between `tool-call` and `tool-result` content parts.
- **run-window unique evidence:** `qualifies` (overridden by the runtime-generated failure above). Mock provider can be configured to produce unique values within a run.

#### Path B — Vercel as provider-wrapper (cassette + real provider)

This path uses cassette-replay against a real provider (e.g. `api.openai.com`
or `api.anthropic.com`) through the Vercel AI SDK layer. It is recorded for
completeness but is **not a selection path** because the identity-bearing
runtime is then the underlying provider, not Vercel.

| # | Requirement | Outcome | Evidence |
|---|---|---|---|
| 1 | Offline execution | qualifies | Cassette-replay at the HTTPS layer (e.g. `nock` or MSW for TypeScript). API key only during one-time cassette recording. Restrictions: non-streaming `generateText` only, SDK version pin, cassette request/response shape stability, maintainer-controlled re-record. |
| 2 | Stable identity | insufficient evidence | Identity is provider-derived (inherits the provider's level-3 evidence profile). Vercel AI SDK adds a documented anti-property on `run-window unique`: per [vercel/ai issue #7727](https://github.com/vercel/ai/issues/7727), the SDK does not guarantee `toolCallId` uniqueness across multiple `streamText` calls. Issue body confirms that some providers (e.g. Google Gemini Flash) reuse IDs across invocations and that Vercel AI SDK does not defensively harden against this at the SDK layer. The SDK-level anti-evidence is scoped to Vercel's own uniqueness guarantee, **not** a claim that underlying provider IDs are non-unique in all configurations. |
| 3 | Comparable surface | not yet evaluated | Path B is not the selection path; full row evaluation is unnecessary. |
| 4 | Deterministic dependency lock | not yet evaluated | Same as row 3. |
| 5 | Linux/eBPF fit | not yet evaluated | Same as row 3. |
| 6 | Small event shape | not yet evaluated | Same as row 3. |
| 7 | Evidence boundary fit | not yet evaluated | Same as row 3. |

#### Overall candidate outcome

**Does not qualify as an independent second runtime; provider-wrapper path
remains `insufficient evidence` and is not selected for v0.**

Rationale: path A is the only route in which Vercel AI SDK acts as an
independent runtime, and it fails level-3 on row 2. Path B inherits identity
from the underlying provider, so a hypothetical `qualifies` outcome there
would not select Vercel as the second runtime — it would select the underlying
provider with a Vercel wrapper. That violates the spirit of #1295's
second-runtime question.

**Expected delegated gate for first fixture PR (only filled if overall outcome is `qualifies`):**
Not applicable.

**Notes:** Wrapper-SDK evaluations now show two distinct identity risks:
adapter-side ID synthesis (PydanticAI) and provider-pass-through without
SDK-level uniqueness guarantees (Vercel AI SDK). This is a review warning,
not a categorical rule; future wrapper candidates may still qualify if they
propagate provider IDs in a strict no-fallback, propagate-or-fail mode with
documented binding intent and documented run-window uniqueness. Issue
[vercel/ai#7727](https://github.com/vercel/ai/issues/7727) is anti-evidence
for SDK-level uniqueness guarantees, not for provider ID uniqueness in
general; underlying provider IDs may still satisfy `run-window unique` under
the right configuration even when the wrapper SDK does not guarantee it.

### Candidate: OpenAI TypeScript SDK direct

The OpenAI TypeScript SDK direct path means using
[`openai-node`](https://github.com/openai/openai-node) (the `openai` npm
package) against the Chat Completions API with a custom client tool, not
through `@openai/agents-js` and not through Assistants/Responses higher-level
surfaces. This isolates the smallest possible second-runtime surface using
OpenAI as the underlying provider, paralleling the Anthropic SDK direct and
OpenAI Python SDK direct evaluations above.

| # | Requirement | Outcome | Evidence |
|---|---|---|---|
| 1 | Offline execution | qualifies | Cassette-replay via `nock` or MSW (TypeScript) at the HTTPS layer against `api.openai.com`. Live API key is required only during one-time cassette recording (curation step), never during delegated acceptance. Restrictions: non-streaming `client.chat.completions.create()` only (streaming cassettes complicate determinism); `openai` SDK version pinned via `package.json`/lockfile; cassette request/response shape stable across compatible SDK patch versions; re-recording is maintainer-controlled, analogous to the [`@openai/agents` bump flow in `fixtures-v0.md`](fixtures-v0.md#dependency-upgrade-contract). |
| 2 | Stable identity | insufficient evidence | One of three level-3 sub-conditions passes via public TypeScript SDK JSDoc; two are gaps. See Stable identity detail below. |
| 3 | Comparable surface | not yet evaluated | Row 2 blocks overall outcome; remaining rows recorded as `not yet evaluated` to avoid speculative claims. Re-evaluate when row 2 is unblocked. |
| 4 | Deterministic dependency lock | not yet evaluated | Row 2 blocks overall outcome. |
| 5 | Linux/eBPF fit | not yet evaluated | Row 2 blocks overall outcome. |
| 6 | Small event shape | not yet evaluated | Row 2 blocks overall outcome. |
| 7 | Evidence boundary fit | not yet evaluated | Row 2 blocks overall outcome. |

**Stable identity detail (row 2):**

The candidate identity field is the `id` field on `ChatCompletionMessageFunctionToolCall`
in the [openai-node TypeScript SDK](https://github.com/openai/openai-node/blob/200211197931763ed43b7ff41839b53e4dbfdf6e/src/resources/chat/completions/completions.ts).
The interface is declared with the interface-level JSDoc *"A call to a function tool created by the model."*

- Field name: `ChatCompletionMessageFunctionToolCall.id`
- Source: `tool_calls` array on the assistant message of a Chat Completions API response, accessed via the TypeScript SDK type
- **runtime-generated evidence:** `insufficient evidence`. The TypeScript JSDoc for the field reads literally *"The ID of the tool call."* — identical wording to the Python SDK docstring. The enclosing interface comment *"A call to a function tool created by the model."* is suggestive of model-side creation, but it documents the object as a whole rather than the `id` field specifically. The example value's `call_` prefix matches OpenAI's other server-issued identifier conventions, and the SDK has no documented client-side generation path. However, no public docs sentence or typed SDK annotation explicitly states that the OpenAI API server generates the `id` field. Per the level-3 strict reading codified in the [Stable Identity Checklist](#stable-identity--level-3-checklist), implicit chain-of-reasoning evidence does not satisfy `runtime-generated`.
- **binding-intended evidence:** `qualifies`. The TypeScript JSDoc for [`ChatCompletionToolMessageParam.tool_call_id`](https://github.com/openai/openai-node/blob/200211197931763ed43b7ff41839b53e4dbfdf6e/src/resources/chat/completions/completions.ts) reads literally *"Tool call that this message is responding to."* — identical wording to the Python SDK. The official Function Calling guide demonstrates the binding mechanism in code using the SDK type.
- **run-window unique evidence:** `insufficient evidence`. The TypeScript JSDoc for the `id` field reads only *"The ID of the tool call."* — no "unique" claim, identical to the Python SDK gap. The official Function Calling guide does not provide an explicit uniqueness guarantee for parallel tool calls within a single response. Per the level-3 strict reading, an implicit uniqueness expectation does not satisfy `run-window unique`.

**Expected delegated gate for first fixture PR (only filled if overall outcome is `qualifies`):**
Not applicable. Overall outcome below is `insufficient evidence`, so no first fixture PR may be opened from this evaluation.

**Overall outcome:** `insufficient evidence`

Per the lowest-row-wins rule in [Evaluation Discipline](#evaluation-discipline),
row 2's `insufficient evidence` determines the candidate's overall outcome.
Rows 3-7 remain `not yet evaluated` because evaluating them would not change
the overall outcome and would invite optimistic interpretation. They become
relevant only if both `runtime-generated` and `run-window unique` evidence
improves.

**Notes:** Two OpenAI direct-SDK evaluations now show the same level-3
identity profile: Python (#1301) and TypeScript (this PR) both document
binding intent, but neither provides explicit public evidence for
`runtime-generated` or `run-window unique`. This observation is scoped to
OpenAI direct SDKs and the public documentation reviewed here; it does not
claim anything about other providers.

### Candidate: Gemini Python `google-genai` direct

The Gemini Python `google-genai` direct path means using the
[`google-genai`](https://github.com/googleapis/python-genai) Python SDK (the
official Google Gen AI Python SDK) against the Gemini API with a custom
function tool, not through Vertex AI, not through the legacy
`google-generativeai` SDK, and not through a wrapper framework.

**Model pin:** `gemini-3.5-flash` (stable/GA, listed at
[ai.google.dev/gemini-api/docs/models](https://ai.google.dev/gemini-api/docs/models)).
This evaluation is scoped strictly to this model identifier; preview models
(`gemini-3.1-pro-preview`, `gemini-3-flash-preview`, etc.) and non-Gemini-3
models are out of scope. The "Gemini 3 model APIs" guarantees cited below
apply to `gemini-3.5-flash` as a Gemini 3 family member.

| # | Requirement | Outcome | Evidence |
|---|---|---|---|
| 1 | Offline execution | qualifies | Cassette-replay via [VCR.py](https://vcrpy.readthedocs.io/en/latest/usage.html) at the HTTPS layer against `generativelanguage.googleapis.com`. Live API key is required only during one-time cassette recording (curation step), never during delegated acceptance. `record_mode='none'` guarantees no network calls during fixture execution. Restrictions: non-streaming `client.models.generate_content()` only (streaming cassettes complicate determinism); `google-genai` SDK version pinned via `requirements.txt`/lockfile; `gemini-3.5-flash` model string pinned in fixture configuration; re-recording is maintainer-controlled, analogous to the [`@openai/agents` bump flow in `fixtures-v0.md`](fixtures-v0.md#dependency-upgrade-contract). |
| 2 | Stable identity | qualifies | All three level-3 sub-conditions pass via literal citations in public docs and typed SDK source. See Stable identity detail below. |
| 3 | Comparable surface | qualifies | A `read_file`-style function can be declared via `types.FunctionDeclaration` and called via the model's `tools` parameter, analogous to the S5 fixture's `read_file` MCP tool. The capability class is the same: one filesystem read returning deterministic content. The function-tool emit pattern produces one `functionCall` part in the assistant message and the fixture sends back one matching `functionResponse` — directly comparable to S5's single tool-call binding. |
| 4 | Deterministic dependency lock | qualifies | `google-genai` is distributed via PyPI and supports standard Python dependency-lock workflows (`requirements.txt` with hash pins, `uv.lock`, or `poetry.lock`). Transitive dependencies are bounded by the SDK's published constraints. This is the same dependency-lock substrate as the Anthropic / OpenAI Python direct evaluations. |
| 5 | Linux/eBPF fit | qualifies | The fixture runs as a standard Python subprocess on the delegated `assay-bpf-runner` host. No native extensions, no specialized runtimes, no host services beyond what the existing Python ecosystem requires. The existing cgroup capture model applies without modification. |
| 6 | Small event shape | qualifies | One `generate_content` call with a single function tool produces one `functionCall` part, the fixture sends back one `functionResponse`, and the run completes. No multi-tool, branching, or multi-binding behavior is exercised in v0. This is the same one-binding shape as S5's `tc_runner_policy_001`. |
| 7 | Evidence boundary fit | qualifies | Standard Python subprocess + cassette-replay produces no new evidence categories. The runner normalizer does not need to broaden filters or evidence taxonomy. Loader / locale / dependency-tree paths remain telemetry, not evidence, per the existing [telemetry-versus-evidence rules in `artifacts-v0.md`](artifacts-v0.md#telemetry-versus-evidence). |

**Stable identity detail (row 2):**

The candidate identity field is `FunctionCall.id` in the
[`google-genai` Python SDK `types.py`](https://github.com/googleapis/python-genai/blob/main/google/genai/types.py).
The class is declared with the field-level docstring quoted below.

- Field name: `FunctionCall.id` (and the matching `FunctionResponse.id`)
- Source: `functionCall` part in the assistant message of a `generate_content` response (or its streaming equivalent)
- **runtime-generated evidence:** `qualifies`. Public docs at [ai.google.dev/gemini-api/docs/function-calling](https://ai.google.dev/gemini-api/docs/function-calling) state literally: *"**Important:** Gemini 3 model APIs now generate a unique `id` for every function call. If you are manually constructing the conversation history or using the REST API, when returning the result of your executed function to the model we recommend passing the matching `id` in your `functionResponse`. If you are using the standard Python or Node.js SDKs, this is handled automatically."* The typed SDK source ([`google/genai/types.py`](https://github.com/googleapis/python-genai/blob/main/google/genai/types.py)) declares the field with description *"The unique id of the function call. If populated, the client to execute the `function_call` and return the response with the matching `id`."* and `Field(default=None)` — no adapter-side default factory. The SDK propagates the model-generated id without synthesizing one.
- **binding-intended evidence:** `qualifies`. Public docs state literally: *"**Always map function IDs:** Gemini 3 now always returns a unique `id` with every `functionCall`. Include this exact `id` in your `functionResponse` so the model can accurately map your result back to the original request."* The typed SDK docstring on `FunctionCall.id` reads literally: *"... return the response with the matching `id`."* Both sources document the binding contract explicitly.
- **run-window unique evidence:** `qualifies`. Public docs state literally: *"generate a unique `id` for every function call"* and *"always returns a unique `id` with every `functionCall`."* The typed SDK docstring contains the literal word *"unique"*: *"The **unique** id of the function call."*

**Expected delegated gate for first fixture PR (only filled if overall outcome is `qualifies`):**
`gates=all` per [`second-runtime-plan.md` § Suggested PR Sequence](second-runtime-plan.md#suggested-pr-sequence)
step 4. A narrower gate for the Gemini Python runtime is later coordinated work; do not introduce it as a side effect of the first fixture PR.

**Overall outcome:** `qualifies`

Per the lowest-row-wins rule in [Evaluation Discipline](#evaluation-discipline),
all seven rows qualify; the candidate overall qualifies.

**Notes:** Gemini Python `google-genai` direct is the first evaluated
candidate to clear level-3 stable identity. The differentiator is
documentation explicitness: the public Gemini 3 API docs state that Gemini 3
model APIs generate a unique id for every function call and require the
matching id in the function response, while the typed SDK field preserves
that id without adapter-side generation. This observation is scoped to the
exact `gemini-3.5-flash` model pin and `google-genai` evidence cited here;
it does not generalize to Vertex AI, legacy `google-generativeai`,
TypeScript SDKs, or non-Gemini-3 models.

**Selection ≠ fixture approval.** This evaluation selects the candidate for
the second-runtime line. Opening the first fixture PR under the
[second-runtime plan](second-runtime-plan.md) is a separate step and is
subject to that plan's "Acceptance Criteria For The First Fixture PR" and
the [CI lane contract](ci-lanes.md). Nothing in this evaluation pre-approves
fixture code, dependencies, cassette implementation, or a narrower delegated
gate.

## Selection Outcome

**Selected second runtime candidate: Gemini Python `google-genai` direct, constrained to exact `gemini-3.5-flash` model pin.**

Evaluated candidates and their overall outcomes:

| Candidate | Overall outcome |
|---|---|
| Anthropic SDK direct | `insufficient evidence` |
| PydanticAI | `does not qualify` |
| OpenAI Python SDK direct | `insufficient evidence` |
| Vercel AI SDK | `does not qualify as independent runtime` (path A); provider-wrapper path (B) is `insufficient evidence` and not a selection path |
| OpenAI TypeScript SDK direct | `insufficient evidence` |
| **Gemini Python `google-genai` direct** | **`qualifies` (with `gemini-3.5-flash` model pin)** |

The selection issue
[`#1295`](https://github.com/Rul1an/assay/issues/1295) is satisfied by this
outcome: Gemini Python `google-genai` direct is named here as the selected
second runtime candidate, scoped strictly to the `gemini-3.5-flash` model
identifier and the `google-genai` SDK as cited in the Gemini candidate
evaluation above.

**Selection ≠ fixture approval.** This selection authorizes a future first
fixture PR under [`second-runtime-plan.md`](second-runtime-plan.md). It does
not pre-approve fixture code, runtime dependencies, cassette implementation,
a narrower delegated gate, or any modification to the v0 artifact contracts.
The first fixture PR must independently satisfy the
[Acceptance Criteria For The First Fixture PR](second-runtime-plan.md#acceptance-criteria-for-the-first-fixture-pr)
and the [CI lane contract](ci-lanes.md).

Re-evaluation of an `insufficient evidence` outcome for previously-evaluated
candidates happens in a separate PR that cites new public evidence which
closes the previous gap; it does not happen by reinterpreting the same
evidence more optimistically. The selection of Gemini does not close those
prior evaluations — it makes them historical evidence for the level-3 rule's
properties rather than active candidates.

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
