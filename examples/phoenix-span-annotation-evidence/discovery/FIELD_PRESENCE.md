# P24 Phoenix Span Annotation Discovery Notes

Date: 2026-04-22

Project used for live capture:

- `p24-phoenix-span-annotation`

Capture path:

1. send one OTLP span into a local Phoenix instance
2. add one span annotation through the public Python client
3. retrieve that annotation through the public Python client

The checked-in reduced sample is intentionally retrieve-derived, not
create-derived.

## Why retrieve wins

The create response was much thinner than the retrieve response.

- create response returned only an inserted annotation id
- retrieve response returned the actual annotation body

So the first honest external-consumer seam is not the create response alone.
It is the retrieve shape, reduced further to the smallest bounded artifact.

## Field presence summary

| Field | Create request | Create response | Retrieve response | Reduced v1 | Note |
| --- | --- | --- | --- | --- | --- |
| `span_id` | yes | no | yes | yes as `entity_id_ref` | Primary target anchor |
| `name` | yes | no | yes | yes as `annotation_name` | Direct upstream label |
| `annotator_kind` | yes | no | yes | optional | Observed provenance only |
| `result.label` | yes | no | yes | yes when present | First-class bounded result |
| `result.score` | yes | no | yes | yes when present | First-class bounded result |
| `result.explanation` | yes | no | yes | optional and bounded | Reviewer aid, not required seam core |
| `identifier` | valid: yes / failure: no | no | valid: yes / failure: empty string | only when non-empty | Empty retrieve values are normalized away |
| `metadata` | valid: yes / failure: no | no | valid: object / failure: empty object | no | Raw metadata is not imported inline in v1 |
| `id` | no | yes | yes | no | Upstream annotation id is not part of the first seam |
| `created_at` | no | no | yes | no | Timestamp source is reduced to one canonical `timestamp` field |
| `updated_at` | no | no | yes | yes as `timestamp` | Best single upstream time anchor in this sample |
| `source` | no | no | yes | no | Platform-side provenance detail, not needed in v1 |
| `user_id` | no | no | yes (`null`) | no | Not part of the first seam |

## Important live nuance

The failure annotation was created without `identifier` or `metadata`, but the
retrieve path still materialized:

- `identifier: ""`
- `metadata: {}`

That is exactly why the reduced artifact:

- drops empty optionals,
- forbids raw metadata inline,
- and keeps the lane on one bounded annotation object rather than on raw
  retrieve payload truth.

## Observed vs reduced

Observed and kept:

- one span-scoped target id
- one annotation name
- one bounded result bag
- optional non-empty `annotator_kind`
- optional non-empty `identifier`
- one reduced timestamp

Observed and dropped:

- annotation id
- source
- user id
- raw metadata
- empty identifier
- empty metadata
- split created/updated timestamp pair

The resulting v1 sample is therefore smaller than Phoenix's raw retrieve shape
while still being live-backed.
