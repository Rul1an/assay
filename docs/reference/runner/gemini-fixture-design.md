# Assay-Runner Gemini Fixture Design

> Internal Phase 2B fixture-design note for the selected second-runtime
> candidate. This page is design-only. It does not approve fixture code,
> add runtime dependencies, introduce a cassette implementation, modify
> workflow triggers, or change v0 artifact contracts.

This note records the design discipline the **first Gemini fixture
implementation PR** must satisfy before code is added. The candidate was
selected in
[`second-runtime-candidate-selection.md`](second-runtime-candidate-selection.md#candidate-gemini-python-google-genai-direct)
and approved by issue
[`#1295`](https://github.com/Rul1an/assay/issues/1295) via PR
[`#1305`](https://github.com/Rul1an/assay/pull/1305).

## Status

- **Selected runtime:** Gemini Python `google-genai` direct
- **Model pin:** `gemini-3.5-flash` (stable/GA per
  [ai.google.dev/gemini-api/docs/models](https://ai.google.dev/gemini-api/docs/models))
- **Selection ≠ fixture approval.** This design note codifies the
  constraints the future fixture implementation must meet. It does not
  itself authorize implementation.
- **Delegated gate for first fixture PR:** `gates=all`, per
  [`second-runtime-plan.md` § Suggested PR Sequence](second-runtime-plan.md#suggested-pr-sequence)
  step 4. A narrower Gemini-specific gate is later coordinated work.

## Goal

Produce a deterministic offline Gemini fixture that exercises the same small
read-file capability class as the S5 OpenAI Agents fixture, with sufficient
stability for three-run delegated determinism over the v0 normalized runner
artifacts. The fixture is a runtime measurement target, not a Gemini
showcase.

## Fixture Command Shape

The fixture must conform to the v0 acceptance fixture invocation contract
in [`fixtures-v0.md` § Invocation Contract](fixtures-v0.md#invocation-contract):

```text
<fixture-script> <work-dir>
```

Constraints:

- requires exactly one work-directory argument
- writes deterministic fixture files below the provided work directory
- avoids wall-clock timestamps, random suffixes, hostnames, absolute temp
  paths, or dependency-version strings in evidence-bearing outputs
- keeps temporary control files (cassette path, request/response JSONL)
  outside the measured work directory
- does not move itself between cgroups or spawn detached processes outside
  the measured process tree

Suggested location for the implementation:
`tests/fixtures/runner-spike/gemini-google-genai-agent.sh` plus a Python
script it invokes. Exact file names are an implementation-PR concern; the
shape above is the contract.

## Cassette Strategy

The fixture must satisfy [Row 1 (Offline execution) of the candidate
evaluation](second-runtime-candidate-selection.md#candidate-gemini-python-google-genai-direct)
via checked-in cassette replay. No live model calls during delegated
acceptance.

### Recording (curation step, maintainer-only)

- maintainer obtains a Gemini API key out-of-band (not stored in the
  fixture)
- maintainer runs the fixture once in **record mode** against
  `generativelanguage.googleapis.com` using `gemini-3.5-flash`
- one non-streaming `client.models.generate_content()` call is made; the
  response (`functionCall` part) is recorded into a deterministic cassette
- the API key is never written to disk, environment files, or commit
  history; the recording session is the only point where it touches the
  system

### Replay (delegated acceptance)

- delegated runner invokes the fixture with `record_mode='none'` (VCR.py
  semantics) or equivalent
- zero network calls; cassette is the sole data source
- replay is byte-deterministic across runs

### Checked-in cassette

- cassette is committed under the fixture directory, alongside the script
  it serves
- cassette is human-reviewable plain text (YAML for VCR.py); not binary
- response body in cassette contains the exact `functionCall.id` and
  payload the model produced at recording time

### Re-recording discipline (maintainer-controlled)

- re-recording happens only when `google-genai` is bumped, when the
  Gemini API contract changes, or when the model pin moves
- re-recording is **not** a normal acceptance behavior; it is a curation
  event documented in the PR that bumps the dependency or pin
- analogous to the
  [`@openai/agents` bump flow in `fixtures-v0.md`](fixtures-v0.md#dependency-upgrade-contract)

## Dependency Lock Path

The fixture implementation PR must use **one** of the following lock paths,
chosen for reproducibility:

| Path | Lock file | When to choose |
|---|---|---|
| `pip` + `requirements.txt` with `--hash` pins | `requirements.txt` | minimal additional tooling on the delegated runner |
| `uv` with `uv.lock` | `uv.lock` | if `uv` is already on the delegated runner |
| `poetry` with `poetry.lock` | `poetry.lock` | if `poetry` is already on the delegated runner |

The fixture directory carries its own lock; no workspace-wide Python
dependency is introduced unless the implementation PR explicitly proposes
it (in which case the workspace-dependency-bump path in the CI lane
contract applies).

`google-genai` is the only required runtime dependency for the fixture
implementation itself; transitive dependencies must remain bounded by the
SDK's published constraints.

## Expected SDK Event Shape

The fixture must emit normalized SDK events conforming to
[`assay.runner.sdk_event.v0`](artifacts-v0.md#sdk-events) and to the
[SDK Fixture Contract in `fixtures-v0.md`](fixtures-v0.md#sdk-fixture-contract).

Mapping from Gemini's function-calling flow to v0 SDK events:

| v0 SDK event | Gemini flow trigger |
|---|---|
| `tool_call_started` | assistant message contains a `functionCall` part — fixture observes the call begin |
| `tool_call_completed` | fixture has produced a `functionResponse` and dispatched it back to the model |
| `run_finished` | model returns a non-function-call assistant message and the run ends |

Constraints inherited from the v0 SDK Fixture Contract:

- stable schema string `assay.runner.sdk_event.v0`
- shared `run_id`
- contiguous `seq` values starting at zero
- stable `source` (suggested: `gemini-google-genai-fixture` or similar
  stable identifier; exact string is fixture-instance scope, not contract)
- installed SDK package name and version loaded from `google-genai` package
  metadata
- stable tool name (suggested: `read_file` to match S5's capability class)
- **stable `tool_call_id` on tool-call events, mapped from
  `FunctionCall.id`** — this is the identity that makes the candidate
  qualify for level-3 stable identity

The `tool_call_id` MUST be the value emitted by Gemini in
`FunctionCall.id` during the recorded cassette interaction. The fixture
must not synthesize a `tool_call_id`; if `FunctionCall.id` is absent in a
response, the fixture must fail loudly rather than fall back to a
generated value.

## Expected Policy Event Shape

The fixture should integrate with the existing policy capture path so the
delegated acceptance covers kernel + policy + SDK correlation, parallel to
the S5 OpenAI Agents fixture.

Constraints inherited from the
[Policy Fixture Contract](fixtures-v0.md#policy-fixture-contract):

- call the intended MCP tool (`read_file`)
- deterministic JSON-RPC ids
- pass a stable `_meta.tool_call_id` for SDK-to-policy correlation
- the policy `tool_call_id` MUST equal the SDK `tool_call_id` (which
  equals `FunctionCall.id` from the cassette)
- write policy decisions to `ASSAY_RUNNER_POLICY_DECISION_LOG`
- one normalized policy event in the captured stream
- policy decision: `allow:read_file` (same coarse outcome as S5)

The implementation PR may share the existing MCP file server used by the
S5 fixture, or provide a Gemini-specific wrapper if scope reasons require.
That is a design decision for the implementation PR; both shapes satisfy
this design note.

## Expected Normalized Artifacts

After three-run determinism comparison, the fixture must produce the same
v0 artifact family as S5:

- `observation-health.json`
- `capability-surface.json`
- `correlation-report.json`
- `layers/sdk.ndjson`
- `layers/policy.ndjson`

### `observation-health.json` expected shape

- `schema = assay.runner.observation_health.v0`
- `platform = linux`
- `kernel_layer = complete`
- `ringbuf_drops = 0`
- `policy_layer = present`
- `sdk_layer = self_reported`
- `cgroup_correlation = clean`
- notes include `s5_sdk_capture: sdk_events=3 sdk_tool_calls=1`

### `capability-surface.json` expected shape

- exactly one normalized filesystem path under the work directory (the
  `read_file` target)
- `mcp_tools` contains `read_file`
- `policy_decisions` contains `allow:read_file`

### `correlation-report.json` expected shape

- `status = clean`
- `ambiguities = []`
- exactly one binding, where `tool_call_id` equals the cassette's recorded
  `FunctionCall.id`
- `policy_decision = allow`
- `window = {"start": "run_started", "end": "run_finished"}`

### Three-run determinism

Three sequential runs of the fixture, via a delegated wrapper script
analogous to
`scripts/ci/runner-spike-openai-agents-kernel-policy-three-run-determinism.sh`,
must produce byte-identical artifacts in the five files listed above.

## Expected Delegated Gate

`gates=all` per [`second-runtime-plan.md` § Suggested PR Sequence](second-runtime-plan.md#suggested-pr-sequence)
step 4. A narrower Gemini-specific gate (e.g. `gemini-kernel-policy`) is
later coordinated work that requires updates to
[`ci-lanes.md`](ci-lanes.md), the lane-check classifier, the workflow
`inputs.gates` enum, and the matching acceptance scripts. **Not a side
effect of the first fixture PR.**

## Kill Criteria (Before Code)

Stop the implementation line **before** writing fixture code if any of these
become true during PR design or implementation:

- Gemini's actual `gemini-3.5-flash` function-calling response does not
  contain a `FunctionCall.id` for the recorded call — the level-3
  qualifies outcome rests on this guarantee
- the `google-genai` SDK silently substitutes a missing id (contradicting
  the typed source `Field(default=None)` evidence used in the candidate
  evaluation)
- cassette determinism cannot be achieved without per-request header
  scrubbing that would also mask the recorded `FunctionCall.id`
- `gemini-3.5-flash` is moved to deprecated, removed, or its function-call
  identity guarantee is rescinded by Google between selection and
  implementation
- dependency installation (`google-genai` and transitive packages) is not
  byte-deterministic enough for three-run normalized-artifact stability
- the fixture cannot satisfy the v0 artifact-shape expectations without
  weakening the runner normalizer's evidence-versus-telemetry filters

If any kill criterion fires, the implementation PR must stop and either
(a) document the regression in a follow-up evaluation PR that updates the
Gemini candidate outcome in
[`second-runtime-candidate-selection.md`](second-runtime-candidate-selection.md),
or (b) open a separate decision PR for the relevant follow-up issue.

## Non-Goals

This design does not:

- approve fixture implementation code
- add runtime dependencies in the docs tree
- add cassette content or cassette-format choice as a contract decision
- modify the v0 artifact contracts, fixture v0 contract, CI lane contract,
  or boundary map
- introduce a narrower delegated gate
- propose cross-runtime capability-diff against S5 (Phase 2C per
  [`second-runtime-plan.md` § Out Of Phase 2B Scope](second-runtime-plan.md#out-of-phase-2b-scope))
- broaden the runner normalizer's evidence taxonomy
- pre-approve later Gemini model bumps or family expansion beyond
  `gemini-3.5-flash`
- close any candidate-selection re-evaluation by removing prior `insufficient
  evidence` entries from the selection note

## Implementation PR Acceptance Checklist

The implementation PR must independently satisfy:

- [ ] [`second-runtime-plan.md` § Acceptance Criteria For The First Fixture PR](second-runtime-plan.md#acceptance-criteria-for-the-first-fixture-pr)
- [ ] [`fixtures-v0.md` § Adding Or Changing A Fixture](fixtures-v0.md#adding-or-changing-a-fixture)
- [ ] [`ci-lanes.md`](ci-lanes.md) — delegated proof recorded with run URL, head SHA, gate, and proof-pack artifact name
- [ ] this design note's Cassette Strategy, Dependency Lock Path, SDK / Policy event shapes, Normalized Artifacts, and Kill Criteria
- [ ] the lane-check classifier correctly routes the PR to `gates=all`

## References

- [Runner artifact v0 contracts](artifacts-v0.md)
- [Runner acceptance fixture v0 contract](fixtures-v0.md)
- [Runner CI lane contract](ci-lanes.md)
- [Runner second runtime Phase 2B plan](second-runtime-plan.md)
- [Runner second runtime candidate selection](second-runtime-candidate-selection.md)
- [Runner capability-diff v0 contract](capability-diff-v0.md)
- [Assay-Runner boundary and extraction map](boundary-map.md)
- [Phase 1 delegated proof pack](proof-packs/phase1-delegated-2026-05-21.md)
- Candidate selection PR: <https://github.com/Rul1an/assay/pull/1305>
- Selection issue (closed by #1305): <https://github.com/Rul1an/assay/issues/1295>
