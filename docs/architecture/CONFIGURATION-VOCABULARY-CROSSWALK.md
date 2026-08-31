# Configuration vocabulary crosswalk

**Generated** by `scripts/docs/generate-configuration-vocabulary-crosswalk.py`. Do not
hand-edit: re-run it instead, or the map goes stale silently, which is the failure it
exists to prevent.

Derived from the committed record corpus by that script. It deliberately records **no**
commit stamp: this file is regenerated and committed by the docs workflow, so a stamp would
name the commit before its own, making the file permanently stale against `--check`.
Freshness is enforced by re-running, not by a date.

Several record schemas here carry a digest or version pinning *what was in force* when a
tool decision was made, under different field names. Nothing else says how they relate, so
a reader who meets one of them can reasonably assume the others mean the same thing. They
do not.

**No claim in this codebase depends on configuration** — the claim gate knows
`PositiveExistence`, `ExhaustiveSet` and `BoundedNegative`, all about observation coverage.
This is a legibility map, not a correctness mechanism, and it justifies no code change.

Field subjects below are read from the producing code, never inferred from the field name.
Inferring from names is exactly the error this page prevents.

## Schemas found in the corpus

| schema | documents | configuration key | populated | what it is a statement about |
|---|---|---|---|---|
| `assay.tool_decision_surface.v0` | 10 | `server.declared_manifest_digest` | 10/10 | The MCP server's declared tool manifest — what the server advertised it could do. The server compares against it and can report `declared_manifest_digest_mismatch`. |
| `assay.tool_decision_truth.otel_projection.v0` | 1 | `spans[].attributes.assay.tdt.declared_policy_digest` | 2/2 | The same fact as `assay.tool_decision_truth.v0`, carried as OpenTelemetry span attributes. |
| `assay.tool_decision_truth.v0` | 1 | `declared_policy_digest` | 1/1 | The declared constraint set the decision was taken under: `McpPolicy::declared_constraint_digest_experimental`, binding tool name, args schema, identity, classes, approval, scope and redaction. Decision identity is the pair `(observed_input_digest, declared_policy_digest)`. |
| `assay.tool_decision_truth.vectors.v0` | 1 | `policies.<name>.version` | 1/1 | A named policy variant a vector exercises. A version label, not a digest over content: comparable for identity between records sharing a naming scheme, not recomputable from bytes. |

## How they relate

Stated as relations with a **direction**, not as equality. Only one pair below earns
"equivalent", and only one way.

| pair | relation | direction | note |
|---|---|---|---|
| `assay.tool_decision_truth.v0` → `…otel_projection.v0` | projection | one way only | The span attributes carry the same fact. Reconstructing the carrier from the projection is **not** claimed: a projection may drop fields. |
| `assay.tool_decision_surface.v0` ↔ `assay.tool_decision_truth.v0` | different subjects | no derivation either way | A server tool manifest is what the server advertised; a declared constraint set is the rule the decision was measured against. Both answer "what was in force", about different things. |
| `assay.tool_decision_truth.vectors.v0` ↔ any digest field | different kind of statement | no derivation | A version names a variant; a digest commits to content. |

## Declared in a type, instantiated nowhere

**PayloadToolDecision (assay-evidence)** — `policy_digest, policy_snapshot_digest (+_alg/_canonicalization/_schema), tool_definition_digest (+_alg/_canonicalization/_schema/_source), args_schema_hash`

All optional, no doc comments, and populated by no committed fixture. Semantics are stated nowhere and there is no instance to read them from, so **no relation to any field above can be asserted**. `policy_digest` reads like a shorter `declared_policy_digest`; that resemblance is a name, not evidence.

## Rules this map follows

- **Not stated is a finding, not a gap.** A schema with no curated subject is emitted as
  such rather than omitted.
- **A mapping is itself a claim.** Saying two fields are the same fact needs a stated
  relation and a direction.
- **No new vocabulary.** This map references what exists and mints nothing.
