# assay project-enforcement-health

Project one existing `assay.enforcement_health.v0` or `assay.enforcement_health.v1`
document into a lossy observation.

```bash
assay project-enforcement-health --format json --input PATH
```

`--input` is required. No input means no projection document and no claim.

## Output

On success, stdout is exactly one `assay.enforcement_health_projection.v0` document:

```json
{"schema":"assay.enforcement_health_projection.v0","lossy":true,"source_schema":"...","observation":"applied|degraded|not_requested"}
```

`source_schema` is the input identity. Mapping:

| source | source value | observation |
|---|---|---|
| v0 | `active` | `applied` |
| v0 | `failed` | `degraded` |
| v0 | `absent` | `not_requested` |
| v1 | `active` | `applied` |
| v1 | `failed` | `degraded` |

`active` maps to `applied` only when the supporting producer invariants hold
(v0 confirmed attach and `strong`; v1 both Landlock confirmations, `strong`,
and no failure). Constructor-illegal `active`, v0 `not_applicable`, unknown
schema or status, forged v1 `absent`, and malformed, missing, or oversized
input exit nonzero with empty stdout.

## Normative

Absence of a projection is **no claim**. It is never a pass. This command does
not prove enforcement efficacy, egress absence, or a sealed sandbox.

The projection checks coherence of an already-typed document, not authenticity
or provenance. A coherent handwritten carrier can therefore project.
