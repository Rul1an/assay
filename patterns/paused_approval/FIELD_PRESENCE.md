# Paused Approval Field Presence

This note records the current observed-versus-derived posture for the paused
approval pattern.

The goal is simple: keep raw capture material separate from the reduced
pause-only artifact so later helper code has a stable boundary to target.

## Source posture

This pattern is anchored to the currently documented `P22` paused approval
seam under
[`examples/openai-agents-js-approval-interruption-evidence`](../../examples/openai-agents-js-approval-interruption-evidence/README.md).

The raw fixtures in this directory are intentionally fixture-sized and
reviewable. They are not claims that Assay now stores or exports full runtime
state as canonical evidence.

## Raw fixture inputs

- [`fixtures/raw/openai_agents_js.paused_result.json`](./fixtures/raw/openai_agents_js.paused_result.json)
- [`fixtures/raw/openai_agents_js.serialized_state.txt`](./fixtures/raw/openai_agents_js.serialized_state.txt)

## Presence table

| Artifact field | Status | Source | Notes |
|---|---|---|---|
| `timestamp` | observed | raw paused result | Comes from the paused result envelope. |
| `pause_reason` | fixed pattern constant | pattern | v1 stays on `tool_approval` only. |
| `interruptions[*].tool_name` | observed, reduced | raw interruption item | Accept reduced aliases such as `toolName` / `tool_name`. |
| `interruptions[*].call_id_ref` | observed, reduced | raw interruption item | Capture may accept `call_id`, `tool_call_id`, `tool_use_id`, `id`, or `rawItem.callId`, but reduces to canonical `call_id_ref`. |
| `interruptions[*].agent_ref` | observed, reduced optional reviewer aid | raw interruption item | Only included when naturally present. |
| `active_agent_ref` | observed, reduced optional reviewer aid | raw paused result | Optional. |
| `last_agent_ref` | observed, reduced optional reviewer aid | raw paused result | Optional. |
| `metadata_ref` | observed, reduced optional reviewer aid | raw paused result | Optional and opaque only. |
| `resume_state_ref` | derived | serialized paused state | Derived fingerprint over serialized paused state; not a native runtime field. |
| `policy_snapshot_hash` | tolerated extension | caller-supplied | Reviewer aid only; not part of the pattern minimum. |
| `policy_decisions` | tolerated extension | caller-supplied | Reviewer aid only; not part of the pattern minimum. |
| `interruptions[*].arguments_hash` | tolerated extension | caller-supplied | Lets richer lanes point at arguments without importing them inline. |

## Explicitly not part of the canonical artifact

These stay out of the canonical pause-only artifact even if a runtime exposes
them:

- raw serialized state
- transcript history
- session identifiers
- `newItems`
- provider continuation hints such as `lastResponseId`
- resolved approval decision data
- raw tool arguments

## Observed vs derived

Observed and derived fields must stay visibly different:

- observed: pause state, bounded interruptions, naturally present reviewer aids
- derived: `resume_state_ref`

That distinction is the pattern. If later helper code blurs it, the pattern has
grown too large.
