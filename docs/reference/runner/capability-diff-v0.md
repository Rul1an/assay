# Assay-Runner Capability-Diff v0 Contract

> Internal Phase 2B contract. This page defines the first Assay-Runner
> capability-diff projection over normalized runner artifacts. It is not a
> runner-emitted archive artifact, not a CLI surface, and not a product
> release contract.

The v0 capability diff answers one narrow question:

```text
Given two clean normalized runner evidence sets, what observed capability
surface changed?
```

The diff is descriptive. It reports added, removed, and unchanged normalized
capability values. It must not decide whether a change is acceptable; that
remains policy, reviewer, or Harness responsibility.

## Inputs

A v0 diff compares two evidence sets, named `base` and `head`. Each set must
provide these normalized artifacts:

| Artifact | Role |
|---|---|
| `observation-health.json` | Determines whether the evidence set is clean enough for a clean diff |
| `capability-surface.json` | Provides the observed capability sets to compare |
| `correlation-report.json` | Provides stable binding identity through `tool_call_id` |

The input artifacts retain their own schema contracts:

- [`observation-health.v0`](artifacts-v0.md#observation-healthjson)
- [`capability-surface.v0`](artifacts-v0.md#capability-surfacejson)
- [`correlation-report.v0`](artifacts-v0.md#correlation-reportjson)

Raw kernel telemetry, workflow logs, proof-pack metadata, and normalized layer
streams are diagnostic context only. They are not primary v0 diff inputs.

The v0 capability diff is a pure projection over normalized evidence. Workflow
run URLs, commit SHAs, and generation timestamps are intentionally not part of
this schema. Consumers that need forensic anchoring should pair the diff with a
proof-pack manifest, which carries workflow context separately.

## Contract Principles

1. **Normalized evidence only.** The diff consumes normalized artifacts after
   the runner normalizer has already drawn the evidence boundary.
2. **Surface-level projection.** v0 compares set-like values in
   `capability-surface.json`. It does not attribute each filesystem path,
   process, endpoint, or tool value to an individual binding window.
3. **Stable binding identity.** Clean v0 diffs require stable `tool_call_id`
   values in `correlation-report.json`.
4. **Health remains strict.** `ringbuf_drops > 0`, incomplete cgroup
   correlation, or missing SDK/policy layers must not be softened into a clean
   diff.
5. **Deterministic serialization.** Arrays are stable sorted sets unless this
   contract explicitly says otherwise.
6. **No acceptability judgment.** The diff says what changed, not whether the
   change is allowed for the project.

## Schema

Schema string:

```text
assay.runner.capability_diff.v0
```

Fields:

| Field | Type | Required | Semantics |
|---|---|---:|---|
| `schema` | string | yes | Must equal `assay.runner.capability_diff.v0` |
| `base_run_id` | string | yes | `run_id` from the base evidence set |
| `head_run_id` | string | yes | `run_id` from the head evidence set |
| `status` | enum | yes | `clean`, `partial:health`, `partial:correlation`, `partial:unbound`, or `failed` |
| `preconditions` | object | yes | Machine-readable checks that determine whether the diff can be clean |
| `scope` | object | yes | Declares what evidence domain this diff used |
| `surface` | object | yes | Added, removed, and unchanged capability-surface values by category |
| `binding_ids` | object | yes | Added, removed, and unchanged tool-call binding ids |
| `policy_outcomes` | object | yes | Policy decision changes for stable binding ids |
| `unbound` | object | yes | Evidence buckets that could not be safely compared in v0 |
| `ambiguities` | array[string] | yes | Stable code-prefixed ambiguity strings |
| `notes` | array[string] | yes | Stable code-prefixed human-readable notes |

## Preconditions

`preconditions` records why a diff is or is not clean.

| Field | Type | Required | Clean value |
|---|---|---:|---|
| `base_health_clean` | boolean | yes | `true` |
| `head_health_clean` | boolean | yes | `true` |
| `base_correlation_clean` | boolean | yes | `true` |
| `head_correlation_clean` | boolean | yes | `true` |
| `stable_tool_call_ids_required` | boolean | yes | `true` |
| `stable_tool_call_ids_present` | boolean | yes | `true` |

A health set is clean only when:

- `kernel_layer=complete`
- `ringbuf_drops=0`
- `policy_layer=present`
- `sdk_layer=self_reported`
- `cgroup_correlation=clean`

`sdk_layer=present` is reserved for a future corroborated SDK path in the
artifact contract. This first capability-diff contract accepts only the
currently proven S5 fixture shape: `sdk_layer=self_reported`.

## Scope

`scope` separates preconditions from projection scope.

| Field | Type | Required | v0 value |
|---|---|---:|---|
| `projection` | string | yes | `surface_set` |
| `uses_raw_telemetry` | boolean | yes | `false` |
| `uses_proof_pack` | boolean | yes | `false` |
| `per_binding_capability_values` | boolean | yes | `false` |

`per_binding_capability_values=false` is load-bearing. v0 correlation proves a
stable binding id and kernel-event window, but the current capability-surface
artifact is global to the run. Therefore v0 must not claim that an individual
path, process, endpoint, or tool value belongs to one specific binding.

## Surface Diff

`surface` contains one object per `capability-surface.v0` category:

- `filesystem_paths`
- `network_endpoints`
- `process_execs`
- `mcp_tools`
- `policy_decisions`

Each category object has the same fields:

| Field | Type | Required | Semantics |
|---|---|---:|---|
| `added` | array[string] | yes | Values present in head and absent from base |
| `removed` | array[string] | yes | Values present in base and absent from head |
| `unchanged` | array[string] | yes | Values present in both base and head |

All arrays serialize in stable lexicographic order.

## Policy Decision Consistency

`surface.policy_decisions` reports the changed set of policy decision summaries
regardless of binding identity. `policy_outcomes.changed` reports changed
coarse policy outcomes per stable binding id. These views can diverge only when
bindings are added or removed.

For unchanged binding ids, the two views must stay consistent. If an unchanged
`tool_call_id` in `binding_ids.unchanged` has a policy summary change that
appears in `surface.policy_decisions.added` or
`surface.policy_decisions.removed`, that same binding must appear in
`policy_outcomes.changed`. Implementations must verify this consistency before
emitting `status=clean`.

## Binding Id Diff

`binding_ids` compares the set of `tool_call_id` values from clean correlation
bindings.

| Field | Type | Required | Semantics |
|---|---|---:|---|
| `added` | array[string] | yes | Binding ids present in head and absent from base |
| `removed` | array[string] | yes | Binding ids present in base and absent from head |
| `unchanged` | array[string] | yes | Binding ids present in both base and head |

`binding_ids.unchanged` reports identity stability only. Policy outcome changes
for stable binding ids are tracked separately in `policy_outcomes.changed`.

Clean v0 does not support order-based fallback. If a binding lacks a stable
`tool_call_id`, the diff is at least `partial:correlation`.

## Policy Outcomes

`policy_outcomes.changed` records changed coarse policy outcomes for unchanged
binding ids.

Each entry has:

| Field | Type | Required | Semantics |
|---|---|---:|---|
| `tool_call_id` | string | yes | Stable binding id whose policy outcome changed |
| `base` | string or null | yes | Base coarse policy outcome |
| `head` | string or null | yes | Head coarse policy outcome |

Entries serialize by `tool_call_id`. v0 accepted fixtures use coarse outcomes
such as `allow` and `deny`; capability-surface policy summaries remain strings
such as `allow:read_file`.

## Unbound Evidence

`unbound` uses the same category names as `surface`, each as a stable
array[string].

For a clean v0 diff, every `unbound` category must be empty. A future input may
make per-value unbound evidence explicit. Until then, v0 producers must not
invent per-binding path attribution from global capability-surface values.
Because all current `capability-surface.v0` values are run-global,
`partial:unbound` is reserved for a future per-binding capability artifact. v0
implementations must keep `unbound` arrays empty; inputs that suggest per-value
unbinding without an explicit versioned source should produce `status=failed`,
not an invented `partial:unbound` projection.

## Status Semantics

| Status | Semantics |
|---|---|
| `clean` | All preconditions are true, all required artifacts validate, correlation is clean for both sides, and all `unbound` arrays are empty |
| `partial:health` | At least one evidence set can be parsed but has incomplete health such as ring-buffer drops or incomplete cgroup correlation |
| `partial:correlation` | Health is sufficient to parse, but at least one correlation report is partial, ambiguous, or lacks stable binding identity |
| `partial:unbound` | Reserved for a future per-binding capability artifact; v0 producers must not emit this status from run-global capability-surface values |
| `failed` | Required artifacts are missing, schema strings are unsupported, run ids are internally inconsistent, or deterministic parsing fails |

`partial:*` diffs may be useful for triage, but they are not clean evidence for
acceptance. `ringbuf_drops > 0` always prevents `status=clean`.

## Idempotence

The first read-only validation gate is idempotence:

```text
diff(S5_acceptance, S5_acceptance)
```

This must produce:

- `status=clean`
- empty `added` and `removed` arrays for every surface category
- `unchanged` arrays equal to the input capability-surface sets
- empty `binding_ids.added` and `binding_ids.removed`
- `binding_ids.unchanged=["tc_runner_policy_001"]`
- empty `policy_outcomes.changed`
- empty `unbound` arrays
- empty `ambiguities`

The golden shape for this case is
[`golden/capability-diff-s5-idempotent-v0.json`](golden/capability-diff-s5-idempotent-v0.json).
The read-only validation entry point is
`scripts/ci/assay_runner_capability_diff_validate.py`; it projects the diff from
the existing S5 golden artifacts and compares it to that frozen output shape.

## Notes Vocabulary

`notes` follows the same code-prefixed convention as the runner artifact
contracts. v0 reserves the `capability_diff_` prefix. The first golden shape
emits `capability_diff_idempotent` when base and head evidence sets are
identical. Implementations must not introduce new note codes without updating
this contract.

## Non-Goals

v0 does not include:

- declared-capability input; the future declared-capability contract is a
  separate Phase 2C+ slice
- per-binding path/process/endpoint attribution
- call-id-less order fallback
- second runtime support
- macOS runner support
- raw telemetry diffing
- proof-pack ingestion as a required input
- OTel or GenAI semantic-convention mapping
- acceptability or policy judgment

## Implementation Placement

The [boundary map](boundary-map.md) places capability-diff projection semantics
on the Trust Basis / Harness side while Runner delivers clean measured-run
input bundles. A future implementation may start as an Assay-side reference
checker, but it must not silently move artifact meaning into the runner
candidate.
