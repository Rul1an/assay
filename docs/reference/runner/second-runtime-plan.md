# Assay-Runner Second Runtime Phase 2B Plan

> Internal Phase 2B planning note. This page defines the entry discipline for
> adding a second deterministic runtime fixture after the first capability-diff
> consumer. It is not a runtime selection record, not a dependency bump request,
> and not a new artifact contract.

The first Phase 2B capability-diff line now has one clean consumer:
`scripts/ci/assay_runner_capability_diff_validate.py` can project
`assay.runner.capability_diff.v0` from explicit normalized evidence
directories. The delegated workflow also uploads a first-class proof pack
during the run.

That makes a second runtime fixture eligible to plan, but not yet eligible to
implement broadly. The next slice should answer one narrow question:

```text
Can a second offline runtime fixture produce the same v0 normalized runner
artifacts cleanly enough for capability-diff comparison?
```

## Entry Conditions

Do not start implementation until these are true:

- `assay.runner.capability_diff.v0` is the active diff contract.
- The reference projection can produce `status=clean` for the accepted S5
  fixture.
- Delegated proof packs are uploaded during `Runner Spike Delegated` runs.
- The candidate runtime has a deterministic offline fixture path with no live
  LLM calls and no live secrets.
- The candidate runtime can expose stable tool-call identity, or the PR stops
  before implementation and opens a correlation-contract decision.

## Candidate Requirements

A candidate runtime must satisfy all of these before code is added:

| Requirement | Rule |
|---|---|
| Offline execution | The fixture must run without network model calls, hosted credentials, or mutable external services |
| Stable identity | Each observed tool call must have a stable id that can map into `tool_call_id` |
| Comparable surface | The first fixture should exercise the same small read-file capability class as S5 |
| Deterministic dependency lock | Runtime dependencies must be pinned or vendored through the existing dependency-review discipline |
| Linux/eBPF fit | The fixture must run on the delegated Linux host under the existing cgroup capture model |
| Small event shape | The fixture should produce one binding first; multi-tool or branching traces are later work |
| Evidence boundary fit | The normalizer must not broaden evidence boundaries to make the runtime look comparable |

If stable identity is absent, do not add order-based matching in the second
runtime PR. That decision belongs in a separate correlation fallback contract,
not in fixture plumbing.

## Expected Artifact Shape

The first second-runtime fixture should produce the existing normalized runner
artifact family. Three-run determinism inherits the
[fixture v0 contract](fixtures-v0.md#three-run-determinism-contract) and
compares the same five files byte-for-byte:

- `observation-health.json`
- `capability-surface.json`
- `correlation-report.json`
- `layers/sdk.ndjson`
- `layers/policy.ndjson`

These artifacts retain their existing schemas; the second runtime fixture must
emit shapes that pass the v0 contracts in
[`artifacts-v0.md`](artifacts-v0.md) without proposing schema extensions.

The expected clean health bar remains unchanged:

- `kernel_layer=complete`
- `ringbuf_drops=0`
- `policy_layer=present`
- `sdk_layer=self_reported` unless a separate contract proves corroborated SDK
  observation
- `cgroup_correlation=clean`

The first capability diff involving the second runtime should be descriptive
only. It may compare the accepted S5 fixture against the new fixture, or compare
two runs of the new fixture, but it must not decide whether any difference is
acceptable.

## Suggested PR Sequence

1. Land this entry plan.
2. Add a candidate-selection note that records the chosen runtime, identity
   source, offline fixture strategy, dependency lock path, and expected gate.
3. Add the smallest fixture instance and local validators without changing the
   v0 artifact contracts.
4. Run delegated proof with `gates=all` for the first fixture PR. A narrower
   named gate for the second runtime is a separate later change that requires
   coordinated updates to:
   - [`ci-lanes.md`](ci-lanes.md) decision table and required-gate mapping
   - the lane-check classifier in `scripts/ci/assay_runner_lane_check.py`
   - the `Runner Spike Delegated` workflow `inputs.gates` enum
   - the matching `scripts/ci/runner-spike-*` acceptance scripts

   Do not add a narrower gate as a side effect of the first fixture PR.
5. Add capability-diff golden output for `diff(second_runtime, second_runtime)`.
   This is the idempotent acceptance check that mirrors the S5 idempotent
   golden defined by [`capability-diff-v0.md`](capability-diff-v0.md#idempotence).
6. Cross-runtime diff examples comparing the second runtime against S5 are
   **out of Phase 2B scope**. They require a separate Phase 2C contract review:
   what does "same capability" mean across runtimes with different tool naming,
   different SDK event vocabularies, and different binding identity sources?
   Do not introduce cross-runtime diff in the first second-runtime PR.

## Acceptance Criteria For The First Fixture PR

The first implementation PR should satisfy all of these:

- The fixture emits one stable binding id and does not rely on order fallback.
- Three-run determinism covers the same normalized artifact family as S5.
- `assay_runner_capability_diff_validate.py` can project a clean idempotent diff
  for the new fixture.
- The delegated proof-pack artifact contains the new runtime archive, selected
  JSON artifacts, gate log, PASS lines, and manifest entry.
- The PR records a successful `Runner Spike Delegated` run URL, head SHA, gate,
  and proof-pack artifact name.
- No `pull_request`, `push`, or `schedule` trigger is added to the delegated
  self-hosted workflow.

## Kill Criteria

Stop the line before implementation if any of these are true:

- The runtime cannot expose stable tool-call identity without timing or ordering
  inference.
- The fixture needs live model calls, live secrets, or mutable hosted state.
- The runtime requires host privileges or services outside the delegated
  runbook.
- Normalized evidence can only be made clean by weakening ring-buffer,
  cgroup-correlation, or telemetry-versus-evidence rules.
- Dependency installation is not reproducible enough for three-run byte
  determinism.

## Out Of Phase 2B Scope

The following are intentionally deferred to Phase 2C or later. They are
listed here as boundary markers so a future PR cannot quietly absorb them
into second-runtime work:

| Item | Why deferred |
|---|---|
| Cross-runtime capability-diff (`diff(second_runtime, S5)`) | requires a contract for what "same capability" means across runtimes; not a fixture-implementation question |
| Declared-capability input | new artifact category; needs its own schema and contract slice |
| Call-id-less fallback semantics | tracked as a separate correlation-contract decision; must not be introduced as fixture plumbing |
| macOS or Windows measurement | a separate platform spike with its own kill criteria and CI lane contract |
| OTel or GenAI semantic-convention mapping | external mapping surface; must not be introduced before the Linux runner boundary is stable |
| Repository extraction of a runner candidate | boundary-map readiness criteria still apply; this plan does not move them |
| Multi-tool or branching agent traces | second runtime first proves one binding; multi-binding is a follow-up after the idempotent diff lands |

This list does not promise future work. It only prevents quiet scope creep
into the first second-runtime fixture PR.

## Non-Goals

This plan does not:

- choose the second runtime
- add runtime dependencies
- add fixture code
- define call-id-less fallback semantics
- add declared-capability inputs
- add macOS or Windows measurement
- add OTel or GenAI semantic-convention mapping
- change branch protection or delegated workflow triggers

Those require separate contracts or implementation PRs after the candidate
selection record exists.

## Follow-Up After Merge

After this plan lands on `main`, the next discoverable step is a candidate
selection issue separate from this document. The issue compares concrete
runtime candidates against the Candidate Requirements table above. It does
not propose code; it produces the selection note that step 2 of the
Suggested PR Sequence requires.

The Ring-Buffer drop debug follow-up tracked in
<https://github.com/Rul1an/assay/issues/1271> remains independent of this
line. It must not weaken the `ringbuf_drops=0` clean-health bar for any
second-runtime fixture.

## References

- [Runner artifact v0 contracts](artifacts-v0.md)
- [Runner acceptance fixture v0 contract](fixtures-v0.md)
- [Runner CI lane contract](ci-lanes.md)
- [Runner capability-diff Phase 2B plan](capability-diff-plan.md)
- [Runner capability-diff v0 contract](capability-diff-v0.md)
- [Assay-Runner boundary and extraction map](boundary-map.md)
- [Phase 1 delegated proof pack](proof-packs/phase1-delegated-2026-05-21.md)
