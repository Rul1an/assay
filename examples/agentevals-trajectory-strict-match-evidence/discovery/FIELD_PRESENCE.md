# P26 AgentEvals Field Presence

Live discovery run date: **2026-04-24**

Package used for local capture:

- `agentevals==0.0.9`
- Python evaluator path:
  `create_trajectory_match_evaluator(trajectory_match_mode="strict")`

## Discovery artifacts

- `valid.evaluator.inputs.json`
- `valid.returned.result.json`
- `failure.evaluator.inputs.json`
- `failure.returned.result.json`

## What was observed

The strict-match evaluator returned a very small dict in both cases:

```json
{
  "comment": null,
  "key": "trajectory_strict_match",
  "metadata": null,
  "score": true
}
```

and:

```json
{
  "comment": null,
  "key": "trajectory_strict_match",
  "metadata": null,
  "score": false
}
```

That observed shape is the reason this lane is viable. The returned object is
much smaller than the evaluated payloads.

## Input vs returned boundary

The caller-side inputs include:

- `trajectory_match_mode`
- `outputs`
- `reference_outputs`
- local package/version notes

The returned result includes:

- `key`
- `score`
- `comment`
- `metadata`

This lane freezes from the returned result, not from the inputs.

## Reduction choice

The canonical reduced artifact keeps:

- `schema`
- `framework`
- `surface`
- `target_kind`
- `evaluator_key`
- `result.score`

The canonical reduced artifact may keep:

- `result.comment`, but only when it is naturally present, non-empty, short,
  and still small enough to fit the seam honestly

The canonical reduced artifact drops:

- raw `outputs`
- raw `reference_outputs`
- `trajectory_match_mode`
- raw `comment` when it is `null`
- raw `metadata` when it is `null`
- package metadata and any local harness notes

## Why `target_kind` stays but `target_id_ref` does not

`target_kind = "trajectory"` names the evaluation level.

The returned strict-match result did not expose a stable trajectory identifier,
so the canonical v1 artifact does not invent one.

## Malformed line

For v1, artifacts are malformed if they carry:

- raw trajectories or reference trajectories
- evaluator config such as `trajectory_match_mode`
- raw returned wrapper fields instead of the reduced canonical names
- arrays or larger evaluation bundles
- prompt/model/rubric metadata
