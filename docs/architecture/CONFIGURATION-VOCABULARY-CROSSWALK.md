# Configuration vocabulary crosswalk

**Generated** by `scripts/docs/generate-configuration-vocabulary-crosswalk.py`. Do not
hand-edit: re-run it instead, or the map goes stale silently, which is the failure it
exists to prevent.

Derived from the record corpus in the tree by that script. It deliberately records **no**
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

## The mapped vocabularies

`populated` counts **occurrences, not documents**: one record can carry the field several
times, and each is counted. It is matched on the field's final path segment by equality,
never by substring, because `declared_manifest_digest` is a prefix of
`declared_manifest_digest_mismatch` and a loose match reports one field's count beside
another field's name.

| schema | documents | configuration key | populated | what it is a statement about |
|---|---|---|---|---|
| `assay.tool_decision_surface.v0` | 10 | `server.declared_manifest_digest` | 10/10 | The **declared, baselined** tool manifest. `docs/reference/mcp-manifest-drift.md` defines *observed* as the latest fully observed `tools/list` — what the server advertised — and *declared* as the baseline it is compared against, so this names the baseline side. The related finding `declared_manifest_digest_mismatch` is a self-consistency check on that side alone (`recompute(declared.tools) != declared.manifest_digest`), belongs to the manifest-drift records rather than to this schema, and is emitted today only by a test-local reference verifier. |
| `assay.tool_decision_truth.otel_projection.v0` | 1 | `spans[].attributes.assay.tdt.declared_policy_digest` | 2/2 | The same fact as `assay.tool_decision_truth.v0`, carried as OpenTelemetry span attributes. |
| `assay.tool_decision_truth.v0` | 1 | `declared_policy_digest` | 2/2 | The declared constraint set the decision was taken under: `McpPolicy::declared_constraint_digest_experimental`, binding tool name, args schema, identity, classes, approval, scope and redaction. Decision identity is the pair `(observed_input_digest, declared_policy_digest)`. |
| `assay.tool_decision_truth.vectors.v0` | 1 | `policies.<name>.version` | 4/4 | A named policy variant a vector exercises. A version label, not a digest over content: comparable for identity between records sharing a naming scheme, not recomputable from bytes. |

## Carrying configuration, semantics not stated

These records reached the same scope test and carry keys the generator's filter reads as
configuration-ish, and nobody has written down what those keys are a statement about. They
are listed rather than omitted: **not stated is a finding, not a gap.**

The filter is deliberately broad, so expect false positives here — a `policy_decisions`
count is not a configuration basis. That direction is the intended one: a false positive is
visible in this table, while a false negative is a vocabulary nobody ever learns about.
Adding a curated subject moves a row up into the table above, and deciding a row does not
belong is equally good, once the reason is written down somewhere.

No relation is asserted for anything here. A shared field name is not evidence.

| schema | documents | configuration keys it carries |
|---|---|---|
| `assay.coverage_aware_drift.annotation.v0` | 5 | `source_report_schema` |
| `assay.experiment.evidenceref_recompute_consumer.v0` | 1 | `canonicalization_profiles.cbor-deterministic-v1.digest_encoding`, `canonicalization_profiles.cbor-deterministic-v1.digest_prefix`, `canonicalization_profiles.jcs-json-v1.digest_encoding`, `canonicalization_profiles.jcs-json-v1.digest_prefix`, and 5 more |
| `assay.experiment.runner_vs_otel.field_matrix.v0` | 16 | `runner_observation.capability_surface.policy_decisions`, `runner_observation.manifest_digest`, `runner_observation.observation_health.policy_layer`, `summary.manifest_digest_binding`, and 2 more |
| `assay.mcp.tunnel_observed.v0` | 3 | `auth_context.authorization_header_digest`, `evidence_refs[].digest`, `evidence_refs[].request_envelope_digest`, `provider_context.component_version`, and 3 more |
| `assay.runner.capability_diff.v0` | 1 | `policy_outcomes`, `surface.policy_decisions`, `unbound.policy_decisions` |
| `assay.runner.capability_surface.v0` | 16 | `policy_decisions` |
| `assay.runner.correlation_report.v0` | 1 | `bindings[].policy_decision` |
| `assay.runner.cross_runtime_diff.v0` | 1 | `canonicalization.policy_decisions`, `policy_outcomes`, `sdk_metadata.base.sdk_version`, `sdk_metadata.head.sdk_version`, and 2 more |
| `example.placeholder.agt-policy-decision` | 1 | `assayproducerversion`, `data.external_schema` |
| `example.placeholder.mcp-tunnel-observed` | 1 | `assayproducerversion`, `data.external_schema`, `data.observed.auth_context.authorization_header_digest`, `data.observed.evidence_refs[].digest`, and 6 more |
| `protectmcp:decision` | 3 | `payload.policy_digest` |

## How they relate

Stated as relations with a **direction**, not as equality. Only one pair below earns
"equivalent", and only one way.

| pair | relation | direction | note |
|---|---|---|---|
| `assay.tool_decision_truth.v0` → `…otel_projection.v0` | projection | one way only | The span attributes carry the same fact. Reconstructing the carrier from the projection is **not** claimed: a projection may drop fields. |
| `assay.tool_decision_surface.v0` ↔ `assay.tool_decision_truth.v0` | different subjects | no derivation either way | A baselined tool manifest is the surface a server was expected to expose; a declared constraint set is the rule the decision was measured against. Both answer "what was in force", about different things. |
| `assay.tool_decision_truth.vectors.v0` ↔ any digest field | different kind of statement | no derivation | A version names a variant; a digest commits to content. |
| `policy_digest` → `policy_snapshot_digest` | self-describing projection | same value, stated as a MUST | PLAN-P56A (Status: Implemented): `policy_snapshot_digest` is the self-describing projection of the existing `policy_digest`, and in supported decision paths both MUST represent the same digest value while the compatibility field remains present. |
| `policy_digest` → `declared_policy_digest` | projection, whole to part | one way only | The doc comment on `McpPolicy::declared_constraint_digest_experimental` states it: unlike `policy_digest`, which is the whole policy, this projects to the declared-constraint surface only. **The containment runs whole-to-part**, which is the opposite of what the shorter name suggests — an earlier version of this page guessed from the names and inverted it. |
| `protectmcp:decision`'s `policy_digest` ↔ this workspace's `policy_digest` | same name, different producer | no stated relation | The signed receipts under `tests/fixtures/interop/` are a third-party record format carrying a field of the same name. Nothing states that the two are computed the same way, so nothing here asserts they are comparable. |

## Declared in a type, populated by no fixture

**PayloadToolDecision (assay-evidence)** — `policy_digest, policy_snapshot_digest (+_alg/_canonicalization/_schema), tool_definition_digest (+_alg/_canonicalization/_schema/_source), args_schema_hash`

No `.json` or `.ndjson` fixture in the tree populates these, which is why they do not appear in the table above. That absence says nothing about their meaning.

**Instances exist.** `crates/assay-evidence/tests/verify_strict_test.rs` and `crates/assay-evidence/src/types/tests.rs` construct the type with every one of these fields populated and run it through verification.

**The semantics are stated**, in prose, outside the corpus this generator reads: `docs/architecture/PLAN-P56A-POLICY-SNAPSHOT-DIGEST-VISIBILITY-2026q2.md` (Status: Implemented) for the `policy_snapshot_*` cluster, `PLAN-P56B-TOOL-DEFINITION-DIGEST-BINDING-2026q2.md` for `tool_definition_*`, and `docs/architecture/evidence-metrics-mapping.md` for `args_schema_hash`.

The lesson is this page's own rule turned on itself. Searching for populated JSON fixtures and finding none is evidence about **fixtures**, not about semantics. Reading that absence as a gap is the same mistake as reading a field name as a meaning.

## Rules this map follows

- **Not stated is a finding, not a gap.** A schema with no curated subject is emitted as
  such rather than omitted.
- **A mapping is itself a claim.** Saying two fields are the same fact needs a stated
  relation and a direction.
- **No new vocabulary.** This map references what exists and mints nothing.
