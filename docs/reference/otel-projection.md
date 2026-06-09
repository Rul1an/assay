# OTel GenAI + OpenInference projection (`assay.otel_projection.v0`)

A read-only, one-directional, lossy view of assay runtime evidence as OpenTelemetry GenAI attributes
plus an OpenInference `span.kind`, so an OTel/OpenInference backend can read assay evidence without
learning assay's vocabulary.

This is a **mapping, not a rewrite**. assay artifacts are the source of truth; the standard fields are
a projection of them. Nothing is parsed back. The output declares this with two top-level fields that
are part of the contract, not decoration:

- `lossy: true` — the standard fields do not carry everything the assay artifacts do.
- `source_of_truth: "assay artifacts"` — the record is the assay artifact, not this view.

Implemented by `assay_core::otel::projection::project`. Golden fixtures (a committed input and its
expected projection) live in `crates/assay-core/tests/fixtures/otel_projection/`, so an external
reader sees the contract concretely and a mapping drift is caught by a test rather than by prose.

## Versioning

```json
"semconv": { "otel_genai": "1.37.0-development", "openinference": "pinned" }
```

OTel GenAI semantic conventions are still Development upstream (as are the MCP conventions), so the
projection pins a version and flags it. A bump is an explicit change, never a silent reinterpretation.
`openinference` is pinned by name (its span-kind set is stable enough to target).

The pinned version targets the GenAI **agent/tool** span surface (`execute_tool`, `gen_ai.tool.*`),
which is newer than the LLM-**client** span surface the rest of the `otel` module pins at 1.28.0:
`execute_tool` did not exist in 1.28.0, so the two surfaces are pinned independently.

A future slice may also emit the OTel **MCP** semantic conventions (`mcp.*`) for these spans, since
assay's `mcp_tools` are MCP tool calls; the MCP conventions are Development today, so that is a
deliberate later addition, not part of v0.

## Mapping

| assay input | projected as | honesty qualifier |
| --- | --- | --- |
| `capability_surface.mcp_tools[]` | OTel `execute_tool` span (`gen_ai.operation.name=execute_tool`, `gen_ai.tool.name`) + OpenInference `span.kind=TOOL` | `assay.claim_class=observed` |
| `capability_surface.policy_decisions[]` (`<verdict>:<key>`) | OpenInference `span.kind=GUARDRAIL` span + `assay.decision` | `assay.claim_class=observed` |
| `enforcement_health.v0` | a **separate** `span.kind=GUARDRAIL` enforcement span with `assay.enforcement.*` | `assay.claim_class=enforcement` |
| `observation_health.v0` | run-level `resource_attributes` (`assay.observation.*`) | (context, not a claim) |
| `capability_surface` raw sets (endpoints, paths, execs) | run-level `resource_attributes` under `assay.*` | the lossy part, stated |
| declared-vs-observed findings (when supplied) | `span.kind=EVALUATOR` or `GUARDRAIL` | *next slice* |

## The load-bearing rule: enforcement is its own span, never on a tool span

Enforcement is projected as a separate span, not as attributes hung next to a tool span. If a tool
span carried `tool ran` and the enforcement attributes together, a downstream tool reads "tool
executed successfully" and misses that the load-bearing claim was "enforcement was active / blocked /
failed". So enforcement gets its own guardrail-style span carrying `assay.claim_class=enforcement` and
the `assay.enforcement.*` attributes, and **no tool span ever carries an `assay.enforcement.*`
attribute**. This is asserted by a test.

Absence of an enforcement span means no `enforcement_health` was supplied. It does **not** mean
enforcement was absent; that distinction lives in the carrier itself
(`network_enforcement: absent | active | failed | not_applicable`).

## Non-claims

The projection carries its own `non_claims`, and they are the point of doing it this way:

- the standard fields are a view, not the source of truth;
- observed is not enforced (tools are `observed`, enforcement is a separate span);
- absence of an enforcement span is not a claim that enforcement was absent;
- the version is pinned and a bump is explicit.

## CLI

`assay project-otel` emits `assay.otel_projection.v0`, a lossy, read-only projection of assay evidence
into selected OTel GenAI / OpenInference-shaped fields. It reads files, parses JSON, calls the library
projector, and writes JSON — it is transport only, with no projection logic of its own.

```bash
assay project-otel \
  --capability-surface capability-surface.json \
  --observation-health observation-health.json \
  --enforcement-health enforcement-health.json
# or write to a file (stdout stays empty on success):
assay project-otel --capability-surface capability-surface.json --out projection.json
```

Only `--capability-surface` is required; `--observation-health` and `--enforcement-health` are
optional and follow the library signature (an absent enforcement-health makes no enforcement claim).
On a read or parse error the command writes a message to stderr and exits non-zero (`2`) with an empty
stdout, and never echoes raw artifact content. It is not a telemetry pipeline: it produces a
projection JSON document, it does not export OTLP, open a network connection, or emit runtime proof.

## Scope

This is the projection function, its fixtures, and the read-only `assay project-otel` CLI wrapper.
There is no OTLP exporter and no live telemetry path yet; those are later slices, kept separate so the
contract and its fixtures stand on their own first.
