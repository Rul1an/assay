# P27 AutoEvals Field Presence

Live discovery run date: **2026-04-24**

Package used for local capture:

- `autoevals==0.2.0`
- Python scorer path: `from autoevals import ExactMatch`

## Discovery artifacts

- `valid.scorer.inputs.json`
- `valid.returned.score.json`
- `failure.scorer.inputs.json`
- `failure.returned.score.json`

## What was observed

The `ExactMatch` scorer returned a `Score` object in both cases:

```json
{
  "error": null,
  "metadata": {},
  "name": "ExactMatch",
  "score": 1
}
```

and:

```json
{
  "error": null,
  "metadata": {},
  "name": "ExactMatch",
  "score": 0
}
```

That observed shape is the reason this lane is viable. The returned object is
much smaller than the compared values.

## Input vs returned boundary

The caller-side inputs include:

- `output`
- `expected`
- local package/version notes

The returned score includes:

- `name`
- `score`
- `metadata`
- `error`

This lane freezes from the returned score, not from the inputs.

## Reduction choice

The canonical reduced artifact keeps:

- `schema`
- `framework`
- `surface`
- `target_kind`
- `scorer_name`
- `result.score`

The canonical reduced artifact drops:

- raw `output`
- raw `expected`
- raw `metadata`, because it was an empty object
- raw `error`, because it was `null`
- package metadata and any local harness notes

## Why `target_kind` stays but `target_id_ref` does not

`target_kind = "output_expected_pair"` names the comparison level.

The returned `ExactMatch` score did not expose a stable target identifier, so
the canonical v1 artifact does not invent one.

## Score shape

For the Python public surface captured here, `ExactMatch` returned integer
scores:

- `1` for exact equality
- `0` for mismatch

The v1 mapper accepts only integer `0` or `1`; it does not accept booleans or
float variants unless a later discovery pass scopes a separate language surface
that naturally returns those shapes.

## Malformed line

For v1, artifacts are malformed if they carry:

- raw `output`, `expected`, or `input`
- raw inline metadata
- raw error state
- Braintrust experiment or dataset wrappers
- scorer configuration
- prompt, model, provider, rubric, or context metadata
- arrays or larger scorer bundles
