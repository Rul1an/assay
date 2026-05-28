# Delegated Semantic-Gap Baseline Plan

> **Status:** delegated-baseline-smoke-verified for Slice 7 of the
> agent-observability fidelity roadmap. This document pinned the one
> delegated `matched_safe_read` sanity run required before any
> semantic-gap finding is promoted beyond synthetic harness behavior.
> The successful smoke record is in
> [`runs/slice7-delegated-baseline/summary.md`](runs/slice7-delegated-baseline/summary.md).
> It does not publish gap findings and does not open the optional OTel
> span-limit study.
>
> **Last updated:** 2026-05-28

## Goal

The delegated baseline asks one narrow question:

```text
When the synthetic matched_safe_read baseline is repeated through real
Runner capture, can the same tool-call id join reported tool intent,
SDK events, policy evidence, and measured filesystem effects without
weakening the claim class?
```

This is a publication gate, not a new semantic-gap experiment. A clean
baseline run lets later semantic-gap findings say "the positive join
path works on delegated Runner capture" before interpreting divergent
scenarios. A failed or inconclusive baseline does not prove a semantic
gap; it means the delegated join path is not yet healthy enough to cite.

## Prerequisites

| Prerequisite | Status | Why it matters |
|---|---|---|
| Full synthetic semantic-gap matrix | Done: [`semantic_gap_harness.py`](semantic_gap_harness.py) emits all six scenarios | The delegated gate should validate the positive baseline path before gap rows are promoted. |
| Evidence-pack carrier | Prototype-ready: [`evidence_pack.py`](evidence_pack.py) | The delegated baseline must be reviewable as a bounded pack or a stable proof-pack reference, not loose files. |
| Runner delegated proof pack | Existing workflow: [`.github/workflows/runner-spike-delegated.yml`](../../../.github/workflows/runner-spike-delegated.yml) | The workflow already runs on the Linux/eBPF host and uploads retained archives, selected JSON, gate logs, and manifest metadata. |
| OpenAI Agents kernel+policy gate | Existing gate: `openai-agents-kernel-policy` | This gate already exercises a deterministic OpenAI Agents tool call, SDK events, policy evidence, kernel capture, and stable `tool_call_id`. |
| Join and claim-class contracts | Reference-ready | The output should reuse `assay.observability.join_result.v0` and `assay.observability.claim_class_cell.v0`, not invent a delegated-only join vocabulary. |

## Baseline Source

Slice 7 should use the existing delegated workflow rather than adding a
new runner lane:

```text
.github/workflows/runner-spike-delegated.yml
inputs.gates = openai-agents-kernel-policy
inputs.build_ebpf = true
```

The selected gate runs
[`scripts/ci/runner-spike-openai-agents-kernel-policy-three-run-determinism.sh`](../../../scripts/ci/runner-spike-openai-agents-kernel-policy-three-run-determinism.sh),
which in turn invokes the acceptance script three times. Each run
captures:

- a Runner archive tarball under the delegated proof pack payload;
- `observation-health.json`;
- `capability-surface.json`;
- `correlation-report.json`;
- `layers/sdk.ndjson`;
- `layers/policy.ndjson`;
- the gate log and pass lines.

The fixture's stable tool-call id is `tc_runner_policy_001`. The
fixture reports a deterministic `read_file` tool call against
`openai-agents-input.txt`, and the Runner archive must observe the same
workdir-bounded file read/open effect.

## Scenario Scope

The first delegated baseline is deliberately one scenario:
`matched_safe_read`. `hidden_write` is the sharpest next delegated gap
candidate, but it should not be added to this slice. It can only be
promoted after the positive baseline is clean or after an inconclusive
baseline has been understood and fixed.

## Smoke Outcome

The follow-up dispatched
[`runner-spike-delegated.yml`](../../../.github/workflows/runner-spike-delegated.yml)
on branch `codex/agent-fidelity-delegated-baseline-smoke` with
`gates=openai-agents-kernel-policy` and `build_ebpf=true`. Run
[`26571739019`](https://github.com/Rul1an/assay/actions/runs/26571739019)
passed all three deterministic OpenAI Agents kernel+policy runs and
uploaded proof pack
`assay-runner-delegated-proof-pack-26571739019` (artifact
`7264883391`, retained until 2026-08-26).

The review record
[`runs/slice7-delegated-baseline/summary.md`](runs/slice7-delegated-baseline/summary.md)
points to the proof-pack reference, a strong `tool_call_id` join result,
claim-class cells, a `positive_join` scenario verdict, and a redaction
manifest. No tarball payload is committed to git; the proof pack remains
the retained GitHub Actions artifact.

## Required Output

A delegated baseline follow-up should produce one stable review
directory or evidence pack with this minimum shape:

| Artifact | Required | Notes |
|---|---|---|
| Delegated proof-pack manifest | Yes | From `assay-runner-delegated-proof-pack-<run_id>/manifest.json`. |
| Runner archive reference or copied tarball | Yes | The archive from `payload/gates/openai-agents-kernel-policy/run-1/` is enough for the first baseline; runs 2 and 3 support determinism. |
| Extracted selected JSON | Yes | `observation-health.json`, `capability-surface.json`, and `correlation-report.json`. |
| SDK events | Yes | `layers/sdk.ndjson`, preserving `tool_call_started` and `tool_call_completed` for `tc_runner_policy_001`. |
| Policy events | Yes | `layers/policy.ndjson`, preserving the policy-side evidence for the same fixture path. |
| Join result | Yes | `assay.observability.join_result.v0` row with `join_key=tool_call_id`, `join_grade=strong`, and `unique_within_scope=true`. |
| Claim-class cells | Yes | Reported intent, measured effect, and joined evidence rows using `assay.observability.claim_class_cell.v0`. |
| Scenario verdict | Yes | `assay.experiment.agent_observability_fidelity.semantic_gap_verdict.v0` with `scenario_id=matched_safe_read`, `verdict=positive_join`, and evidence-pack claim class `positive_join`. |
| Redaction manifest | Yes | Required even when no redaction is applied. |
| Summary | Yes | One-page Markdown with run URL, commit, gate, health verdict, join verdict, and non-claims. |

The delegated proof pack itself is operational evidence. It should not
be reclassified as a Runner archive schema or product evidence-pack
format merely because the baseline plan consumes it.

## Acceptance Rules

- Dispatch exactly the `openai-agents-kernel-policy` delegated gate for
  the first delegated baseline. Do not use `all` unless a separate PR
  explicitly needs additional gates.
- The delegated workflow must pass, including all three deterministic
  OpenAI Agents kernel+policy runs.
- The proof pack must include the `openai-agents-kernel-policy` gate
  with status `passed`, at least one Runner archive tarball, selected
  JSON, and gate log pass lines.
- `observation-health.json` must be clean enough for citation:
  `kernel_layer` complete, cgroup correlation clean, and no Runner
  health failure that would downgrade the join.
- SDK events must include exactly one started/completed tool-call pair
  for `tc_runner_policy_001` and tool `read_file`.
- The measured archive must include a workdir-bounded read/open effect
  for `openai-agents-input.txt`. A target path outside the delegated
  workdir fails the baseline.
- The join result must be strong by `tool_call_id`; timestamp/order may
  appear only as supporting context and may not upgrade the claim.
- If any required artifact is missing, the baseline outcome is
  `inconclusive`, not `semantic_gap`.
- If the delegated baseline is inconclusive, do not publish delegated
  gap findings and do not run delegated gap scenarios until the baseline
  failure is understood.
- The first delegated baseline may cite only the positive join path. It
  does not promote `hidden_write`, `path_rewrite`, `retry_self_correction`,
  `runtime_side_effect`, or `weak_join_fallback` to measured findings.

## Follow-Up Dispatch/Conversion Gate

A follow-up implementation/dispatch pass is complete when it:

1. either adds a small converter that turns the delegated proof pack into
   the required `matched_safe_read` review directory, or documents that
   the existing evidence-pack prototype can carry the proof-pack
   references without code changes. Any converter must reuse the
   evidence-pack, join-result, and claim-class vocabularies rather than
   introducing a new artifact family;
2. dispatches `runner-spike-delegated.yml` with
   `gates=openai-agents-kernel-policy` and `build_ebpf=true`;
3. records the workflow run URL and proof-pack artifact name;
4. validates the join, health, SDK, policy, and measured-effect
   invariants above; and
5. updates the roadmap from `delegated-baseline-plan-ready` to
   `delegated-baseline-smoke-verified` only if the run passes.

If the run fails, the follow-up should record an inconclusive baseline
result and stop. It should not silently widen to more delegated
scenarios.

## Non-Claims

- This plan does not dispatch the delegated baseline.
- This plan does not publish semantic-gap findings.
- This plan does not claim delegated gap scenarios are ready.
- This plan does not promote evidence packs, semantic-gap verdicts, or
  join rows to product APIs.
- This plan does not rank Runner, OTel, OpenInference, or the OpenAI
  Agents SDK.
- This plan does not open the optional OTel span-limit characterization
  issue.
