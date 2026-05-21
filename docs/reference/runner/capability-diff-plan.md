# Assay-Runner Capability-Diff Phase 2B Plan

> Internal Phase 2B planning note. This page defines the first capability-diff
> contract slice to design after the Phase 2A runner contracts. It is not a
> schema contract, not a CLI design, and not a product promise.

Phase 1 proved that the delegated runner can produce deterministic normalized
evidence for one Linux/eBPF OpenAI Agents fixture. Phase 2A made that proof
reviewable through [artifact](artifacts-v0.md), [fixture](fixtures-v0.md),
[CI-lane](ci-lanes.md),
[proof-pack](proof-packs/phase1-delegated-2026-05-21.md), and
[boundary](boundary-map.md) contracts.

The first Phase 2B capability-diff slice should answer a narrower question:

```text
Given two clean normalized runner evidence sets, what capability surface
changed, and what evidence supports that projection?
```

The first slice must stay deliberately small. It should freeze the diff
contract before adding a broad diff engine, second runtime, macOS path, or
public reporting surface.

## Inputs

The v0 diff contract should consume normalized runner artifacts, not raw
telemetry:

| Input | Role |
|---|---|
| `observation-health.json` | Determines whether a diff may be clean, partial, or failed |
| `capability-surface.json` | Provides normalized observed capability sets |
| `correlation-report.json` | Provides stable binding identity through `tool_call_id` |

The following may be retained for diagnostics, but they are not primary v0 diff
inputs:

- raw kernel telemetry
- workflow logs
- proof-pack metadata
- `layers/sdk.ndjson`
- `layers/policy.ndjson`

Proof packs are useful for durable review and future reproduction, but the
first diff contract should remain a pure projection over normalized evidence.

## Fixed Decisions

These decisions are inherited from the Phase 2A contracts:

- clean v0 diff requires `kernel_layer=complete`, `ringbuf_drops=0`,
  `policy_layer=present`, `sdk_layer=self_reported`, and
  `cgroup_correlation=clean`
- clean SDK-to-policy binding requires stable `tool_call_id`
- call-id-less runtimes are out of scope for the first diff contract
- SDK timestamps are informational only and must not become primary join keys
  or ordering fallbacks
- `filesystem_paths` contains full observed path values; prefix grouping is a
  later projection choice, not a rewrite of the artifact contract
- the normalizer owns evidence boundaries; capability diff consumes those
  boundaries and must not broaden them

## Diff Basis

The first contract should compare observed capability surfaces. It should not
claim a full declared-versus-observed gap until there is a versioned declared
capability input.

For the first slice, the safe vocabulary is:

| Category | Meaning |
|---|---|
| `observed` | Present in normalized capability evidence |
| `policy_observed` | Present in normalized policy decision summaries, including allow or deny outcomes |
| `bound` | Connected to a clean `tool_call_id` correlation binding |
| `unbound` | Present in normalized evidence but not connected to a clean binding |

The contract may describe `added`, `removed`, and `unchanged` capability values
between two evidence sets. It must not decide whether a change is acceptable;
that remains policy, reviewer, or Harness responsibility.

## Candidate Output Shape

The next PR may use a candidate shape like this as a draft, but this page does
not freeze it:

```json
{
  "schema": "assay.runner.capability_diff.v0_draft",
  "base_run_id": "run_base",
  "head_run_id": "run_head",
  "status": "clean",
  "basis": {
    "requires_stable_tool_call_id": true,
    "uses_raw_telemetry": false
  },
  "bindings": [
    {
      "tool_call_id": "tc_runner_policy_001",
      "policy_decision": "allow",
      "added": {
        "filesystem_paths": [],
        "network_endpoints": [],
        "process_execs": [],
        "mcp_tools": []
      },
      "removed": {
        "filesystem_paths": [],
        "network_endpoints": [],
        "process_execs": [],
        "mcp_tools": []
      },
      "ambiguities": []
    }
  ],
  "summary": {
    "added_count": 0,
    "removed_count": 0,
    "unbound_count": 0
  },
  "notes": []
}
```

The contract PR should either promote a shape like this to a versioned contract
or deliberately choose a smaller one. The mini-plan does not require this exact
field set. If the contract keeps a `basis` object, it should distinguish
preconditions such as stable binding identity from scope claims such as whether
raw telemetry is used.

## Status Semantics

The first contract should define three states:

| Status | Rule |
|---|---|
| `clean` | Both input evidence sets have complete health, `sdk_layer=self_reported`, clean correlation, internally consistent run ids, and stable binding ids |
| `partial` | At least one input can be parsed but has partial health, partial correlation, or unbound evidence that prevents a clean claim |
| `failed` | Required artifacts are missing, schema strings are unsupported, run ids are inconsistent, or binding identity is unusable |

`ringbuf_drops > 0` must not be softened into a clean diff. A partial diff may
help triage a failed run, but it must remain visibly incomplete.

## Non-Goals

Do not include the following in the first slice:

- second runtime support
- call-id-less order fallback
- macOS runner support
- OTel or GenAI semantic-convention mapping
- raw telemetry diffing
- proof-pack ingestion as a required input
- public reporting language
- extraction into a separate runner repository
- automatic merge or branch-protection changes

## Contract PR Acceptance

The follow-up contract PR should:

1. Add a versioned capability-diff contract page under `docs/reference/runner/`.
2. Add at least one golden or shape example for the diff output.
3. State exactly which normalized artifacts are inputs.
4. Reuse the accepted S5 fixture as the first clean example.
5. Keep `tool_call_id` required for clean binding identity.
6. Preserve the `ringbuf_drops=0` clean-diff rule.
7. Avoid runtime behavior changes unless the PR also updates delegated proof.

Docs-only contract work can use ordinary docs CI. Any PR that changes runner
artifact assertions, acceptance scripts, or executable diff code must follow
the CI lane contract.

## Open Questions

These should be answered by the contract PR, not by this mini-plan:

- Should path projection group `filesystem_paths` by prefix, exact path, or
  both?
- Does v0 diff include process and network categories, or start with
  filesystem plus MCP/tool categories only?
- Where does a future declared-capability input live?
- Is the first implementation a Runner helper, a Harness projection, or an
  Assay-side reference checker?
- Should unbound evidence be represented per category, per binding window, or
  only as a top-level ambiguity?
- Should `partial` remain a single status or become a family such as
  `partial:health`, `partial:correlation`, and `partial:unbound`? These have
  different triage paths, but a single status may be enough for v0.

The boundary map currently places capability-diff projection semantics on the
Trust Basis / Harness side while Runner delivers clean input bundles. The
contract PR may confirm or revisit that placement, but it must not silently
relocate the boundary.

## Suggested Sequence

1. Land this mini-plan.
2. Draft `capability-diff-v0.md` with one golden shape.
3. Add read-only validation: `diff(S5_acceptance, S5_acceptance)` must produce
   `status=clean` with zero added, removed, or unbound entries, and the output
   must validate against the candidate contract schema.
4. Implement a narrow projection only after the contract review settles.
5. Revisit second-runtime fixtures after the diff contract has one clean
   consumer.
