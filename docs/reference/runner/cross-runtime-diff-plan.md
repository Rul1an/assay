# Assay-Runner Cross-Runtime Diff Phase 2C Mini-Plan

> Internal Phase 2C planning note. This page is design-only and intentionally
> small. It does not freeze a schema, propose code, propose a CI lane change,
> or pre-approve a Phase 2C implementation PR.

Phase 2B closed with two qualifying runtime fixtures landed under delegated
`gates=all`:

- the S5 OpenAI Agents fixture (`@openai/agents` SDK)
- the Gemini Python `google-genai` direct fixture (`gemini-3.5-flash` pin)

That makes `diff(S5_fixture, Gemini_fixture)` an empirically reachable
question for the first time. This document records the **scope and
discipline** for opening that question. Cross-runtime diff is explicitly
marked out of Phase 2B scope in
[`second-runtime-plan.md` § Out Of Phase 2B Scope](second-runtime-plan.md#out-of-phase-2b-scope)
and is not covered by the current
[`capability-diff-v0.md`](capability-diff-v0.md) contract, which defines
intra-runtime diff semantics only. This plan is the bridge from those
non-goals to a Phase 2C contract slice.

## Status

- **Phase:** 2C planning (mini-plan, not freeze)
- **Inputs available:** two normalized runner evidence sets from clean
  delegated runs — S5 and Gemini
- **Outputs not yet decided:** none. This plan does not commit to any
  output schema, file shape, or CLI surface.

## Scope

The Phase 2C cross-runtime line, in its first slice, answers exactly:

> *Given two clean normalized runner evidence sets recorded from different
> runtime fixtures (S5 OpenAI Agents and Gemini Python `google-genai` direct),
> what differs in their capability-surface projection, and which of those
> differences are runtime-implementation noise versus capability-surface
> meaningful?*

The scope is intentionally **S5 ↔ Gemini only**. Adding a third runtime,
declared-capability input, or live-LLM cassette regeneration is out of
scope for this slice and belongs in later Phase 2C+ contract decisions.

The scope is intentionally **a projection over existing v0 artifacts**.
No new artifact category, no new schema. Per
[`capability-diff-v0.md`](capability-diff-v0.md), the primary diff inputs
are `observation-health.json`, `capability-surface.json`, and
`correlation-report.json`; `layers/sdk.ndjson` and `layers/policy.ndjson`
are diagnostic context, not required primary inputs. The Phase 2C
cross-runtime slice inherits the same boundary: primary inputs remain
those three artifacts; the layer streams may inform the decision-issue
discussion of the open semantic questions below but are not required
contract inputs.

## Inputs

Two evidence sets, each from a clean delegated `gates=all` run:

| Source | Acceptance script | Cassette | Tool call id source |
|---|---|---|---|
| S5 OpenAI Agents | `scripts/ci/runner-spike-openai-agents-kernel-policy-acceptance.sh` | DeterministicToolCallModel (hardcoded `tc_runner_policy_001`) | fixture-chosen; v0 accepted stable fixture binding id |
| Gemini google-genai | `scripts/ci/runner-spike-gemini-google-genai-acceptance.sh` | `tests/fixtures/runner-spike/gemini-google-genai/cassettes/fixture.yaml` (cassette-recorded `ho0csecf`) | provider-generated; level-3 qualifying per second-runtime selection |

A Phase 2C diff over these two sets has structurally different binding ids
(`tc_runner_policy_001` vs `ho0csecf`), structurally different SDK package
metadata (`@openai/agents` vs `google-genai`), and structurally different
fixture file paths (`openai-agents-input.txt` vs `gemini-input.txt`). The
mini-plan must take a position on which of these structural differences are
*runtime-noise* and which are *capability-surface meaningful*.

## Contract Principles (Inherited)

This slice inherits from `capability-diff-v0`:

1. **Normalized evidence only.** No raw kernel telemetry, no proof-pack
   metadata as primary diff input.
2. **Health remains strict.** Both evidence sets must satisfy the v0
   `observation-health` clean criteria; partial health on either side
   yields `partial:health`.
3. **Stable binding identity, with asymmetric sources.** Both sides must
   have stable binding ids within their own run window, but their identity
   *sources* differ in a way that matters for Phase 2C: S5 uses a
   fixture-chosen `tc_runner_policy_001` (a v0 accepted stable fixture
   binding id, explicitly allowed under the fixture-v0 accepted-instance
   rules); Gemini uses a provider-generated `FunctionCall.id` that meets
   the level-3 rule from
   [`second-runtime-candidate-selection.md`](second-runtime-candidate-selection.md#stable-identity--level-3-checklist)
   (runtime-generated, binding-intended, run-window unique). Cross-runtime
   v0 must explicitly decide how to compare these different identity
   sources — that is part of the central open question recorded below, not
   a property the diff can assume.
4. **No acceptability judgment.** The cross-runtime diff describes what
   differs; it does not declare whether any difference is acceptable for a
   project.
5. **Idempotence remains.** `diff(S5, S5)` and `diff(Gemini, Gemini)` must
   continue to produce `status=clean` with empty added/removed sets as
   defined by `capability-diff-v0`. Cross-runtime diff cannot regress
   intra-runtime idempotence.

## The Central Open Question

The mini-plan **does not answer** this; it only records it for the
follow-up contract PR to settle:

> Which differences between two clean normalized runner evidence sets,
> recorded from different runtimes, are runtime-implementation noise (and
> should therefore be quotiented out before reporting `added`/`removed`)
> and which are genuine capability-surface differences (and should
> therefore appear in the diff output)?

Three concrete categories where the answer is non-obvious:

### A. Fixture file paths

`capability-surface.filesystem_paths` stores **full paths**, so the
S5 and Gemini surfaces differ in two distinct ways at the same time:

1. **Work-dir prefix.** S5 runs under
   `/tmp/assay-runner-openai-agents-kernel-policy/work/...`; Gemini runs
   under `/tmp/assay-runner-gemini-google-genai-kernel-policy/work/...`.
   The prefix differs entirely because each acceptance script uses its
   own `mktemp -d` template.
2. **Fixture-local filename.** S5 writes `openai-agents-input.txt`;
   Gemini writes `gemini-input.txt`. The shared companion file
   `policy-input.txt` has the same name on both sides but a different
   work-dir prefix per (1).

The *underlying capability* (a single `read_file` invocation on a
deterministic work-dir input) is identical, but two layers of path noise
sit between the two surfaces. The Phase 2C contract slice must take a
position on each layer separately:

- **Work-dir prefix layer.** Almost certainly runtime noise — every
  acceptance script picks its own prefix, and quotienting by the prefix
  before comparison is straightforward (replace with a canonical
  `<work>/` placeholder).
- **Fixture-local filename layer.** Genuinely ambiguous — different
  fixtures legitimately pick different file names, but those names also
  appear in policy events and correlation bindings. A path-noise quotient
  here is more invasive than at the prefix layer.

Two interpretations are defensible:

- **Both layers are noise** — quotient prefix and filename, report
  `added=[]`/`removed=[]` if the underlying tool name and decision match
- **Only the prefix layer is noise** — quotient prefix, keep fixture
  filenames as capability-surface differences

The Phase 2C contract slice must pick one combination and defend it.
This mini-plan deliberately does not pick yet.

### B. Tool-call binding ids

S5's `tool_call_id` is `tc_runner_policy_001`, Gemini's is
`ho0csecf`. The binding-id values differ even though both runs exercised
*the same MCP tool* (`read_file`) with *the same policy decision* (`allow`).

Two interpretations:

- **Binding ids are per-run identity tokens** — different across any two
  runs, never directly comparable; the diff should report `unchanged=[]`
  for `binding_ids` even when both runs exercised the same tool
- **Binding ids are stable for capability comparison** — only stable
  within a run; cross-run comparison happens via the *bound tool name*
  and *bound policy decision*, not the id itself

The latter interpretation requires a new derived projection. The
former interpretation makes cross-runtime binding diff trivially "no
unchanged binding ids", which is correct but uninformative.

### C. SDK metadata

S5 emits `sdk_name=@openai/agents`, `sdk_version=0.11.4`. Gemini emits
`sdk_name=google-genai`, `sdk_version=2.6.0`. These differ trivially and
the diff should report them — but as what? As `added`/`removed` entries
in a `sdk_metadata` projection that does not currently exist? Or as
out-of-scope for the capability-surface projection that v0 defines?

This is the **simplest** of the three open questions: SDK metadata is
runtime-implementation, not capability-surface, and likely belongs in a
side-band of the diff rather than as added/removed entries. But the
contract slice has to say so explicitly.

## Non-Goals

This mini-plan does not:

- propose a Phase 2C output schema or schema string
- propose a new file under `docs/reference/runner/golden/`
- propose a CLI command or implementation surface
- decide the central open question above (paths/ids/SDK metadata)
- introduce a third runtime fixture (Anthropic SDK direct,
  PydanticAI strict-mode, Vercel AI SDK provider-wrapper) — those evaluations
  remain at `insufficient evidence` per
  [`second-runtime-candidate-selection.md`](second-runtime-candidate-selection.md)
- declare any policy or acceptability semantics
- modify v0 artifact contracts, fixture v0 contracts, or capability-diff
  v0 contract
- pre-approve cross-runtime live LLM calls or cassette regeneration
  semantics
- propose a delegated gate change or new CI lane
- introduce or modify lane-check classifier rules
- imply that Phase 2C must result in a `qualifies` outcome — `does not
  qualify as v0 cross-runtime semantics` is a legitimate outcome if the
  open questions resolve against meaningful comparability

## Kill Criteria (Before Contract Freeze)

Stop the Phase 2C cross-runtime line before any contract freeze PR if any
of these become true:

1. The central open question (paths/ids/SDK metadata) cannot be answered
   without breaking the v0 `capability-diff` contract
2. Quotienting out path differences requires per-runtime knowledge in the
   diff projection (the diff becomes adapter-laden, not provider-agnostic)
3. Binding-id cross-comparison requires deriving a new identifier scheme
   that itself needs a separate Phase 2C contract slice
4. Adding a third runtime would be required to make the contract
   defensible against "this only works because we have exactly two
   carefully-curated fixtures"
5. The mini-plan keeps the line open for more than two consolidation
   windows without a contract PR landing — that suggests the question
   itself is not yet ready

If any fires, document the outcome in this file and the Phase 2C line
pauses. Do not work around by stretching `capability-diff-v0` to fit.

## Suggested Slice Sequence

Per the Phase 2B sequence template (and what worked for the second-runtime
line):

1. **Land this mini-plan** (this PR — docs only)
2. **Open a contract-design issue** that records the central open question
   and the three interpretation choices. Treat the issue as a decision
   gate the way `#1275` was for call-id-less fallback.
3. **Decide the three open questions in that issue**, with explicit
   arguments per choice. Outcome is a *single* combination of
   path/id/SDK-metadata interpretations.
4. **Open a contract slice PR** that freezes
   `cross-runtime-diff-v0` as a new section in `capability-diff-v0.md`,
   or as a sibling document if the diff schema diverges enough. Include a
   golden shape for the cross-runtime `S5 ↔ Gemini` case.
5. **Open an implementation PR** that extends or wraps
   `assay_runner_capability_diff_validate.py` to project the cross-runtime
   diff and validate against the golden.
6. **No new fixture, no new delegated gate** in any of the above. The
   Phase 2C slice operates entirely over the two cassette-replayed fixture
   outputs that already exist on `main`.

This sequence intentionally mirrors how Phase 2B did selection → design →
implementation, with the same scope-creep prevention.

## Out Of Phase 2C Scope (Phase 2D and Later)

Listed here as boundary markers so a Phase 2C PR cannot quietly absorb
them:

| Item | Why deferred |
|---|---|
| Third-runtime fixture (Anthropic SDK strict-mode, PydanticAI strict-mode, etc.) | Requires re-opening `second-runtime-candidate-selection.md`, separate work |
| Declared-capability input as a Phase 2C addition | Different evidence category; needs its own schema slice |
| Cross-runtime diff over `correlation-report` ambiguities | Phase 2C v0 starts with capability-surface; correlation ambiguity is a richer second slice |
| Cross-platform (macOS, Windows) runner evidence | Out of scope per [`platform-and-extraction-readiness.md`](platform-and-extraction-readiness.md) |
| Cross-runtime diff in delegated `gates=all` as required acceptance | Phase 2C is a projection over existing evidence; not a runtime gate |
| Live cross-runtime regeneration (e.g. nightly re-record both cassettes and diff) | Operational concern; not a contract concern |
| OTel or GenAI semconv mapping of cross-runtime diff output | External mapping surface; unrelated to projection contract |

## References

- [Runner capability-diff Phase 2B plan](capability-diff-plan.md)
- [Runner capability-diff v0 contract](capability-diff-v0.md)
- [Runner second runtime Phase 2B plan](second-runtime-plan.md)
- [Runner second runtime candidate selection](second-runtime-candidate-selection.md)
- [Runner Gemini fixture design](gemini-fixture-design.md)
- [Assay-Runner boundary and extraction map](boundary-map.md)
- [Runner platform and extraction readiness](platform-and-extraction-readiness.md)
- S5 fixture acceptance script: `scripts/ci/runner-spike-openai-agents-kernel-policy-acceptance.sh`
- Gemini fixture acceptance script: `scripts/ci/runner-spike-gemini-google-genai-acceptance.sh`
