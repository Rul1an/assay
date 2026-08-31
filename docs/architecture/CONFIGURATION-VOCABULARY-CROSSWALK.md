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

**The claim gate does not take configuration as an input.** Its claim kinds are
`PositiveExistence`, `ExhaustiveSet` and `BoundedNegative`, and all three are about
observation coverage.

That is a statement about the gate, not about the codebase, and an earlier version of this
page generalised it into "no claim in this codebase depends on configuration". That is
false. ADR-043 conditions an enforcement statement on configuration — *a capability that
cannot bind `declared_policy_digest` makes no enforcement statement in evidence* — and the
decision identity in the table below is the pair `(observed_input_digest,
declared_policy_digest)`, which takes configuration as an input by construction. The false
generalisation mattered because it was what licensed the next sentence.

This page is a legibility map rather than a mechanism: it adds no check and changes no
behaviour. That is a claim about **this page**, and nothing follows from it about what else
in the tree depends on configuration.

Field subjects below are read from the producing code, never inferred from the field name.
Inferring from names is exactly the error this page prevents.

## The mapped vocabularies

`populated` counts **occurrences, not documents**: one record can carry the field several
times, and each is counted. It is matched on the field's final path segment by equality,
never by substring, because `declared_manifest_digest` is a prefix of
`declared_manifest_digest_mismatch` and a loose match reports one field's count beside
another field's name.

| schema | documents | curated key | populated | other keys it carries | what it is a statement about |
|---|---|---|---|---|---|
| `assay.tool_decision_surface.v0` | 10 | `server.declared_manifest_digest` | 10/10 | `observed_tool_decisions[].correlation.source_class`, `observed_tool_decisions[].response.side_effect.verification_subject_digest` | The **declared, baselined** tool manifest. `docs/reference/mcp-manifest-drift.md` defines *observed* as the latest fully observed `tools/list` — what the server advertised — and *declared* as the baseline it is compared against, so this names the baseline side. The related finding `declared_manifest_digest_mismatch` is a self-consistency check on that side alone (`recompute(declared.tools) != declared.manifest_digest`), belongs to the manifest-drift records rather than to this schema, and is emitted today only by a test-local reference verifier. |
| `assay.tool_decision_truth.otel_projection.v0` | 1 | `spans[].attributes.assay.tdt.declared_policy_digest` | 2/2 | `spans[].attributes.assay.tdt.carrier_content_digest`, `spans[].attributes.assay.tdt.decision_identity_digest`, `spans[].attributes.assay.tdt.observed_input_digest`, `spans[].attributes.assay.tdt.source_class` | The same fact as `assay.tool_decision_truth.v0`, carried as OpenTelemetry span attributes. |
| `assay.tool_decision_truth.v0` | 1 | `declared_policy_digest` | 2/2 | `args_digest`, `decision_identity`, `decision_identity.observed_input_digest`, `identity_state`, and 2 more | The declared constraint set the decision was taken under: `McpPolicy::declared_constraint_digest_experimental`, binding tool name, args schema, identity, classes, approval, scope and redaction. Decision identity is the pair `(observed_input_digest, declared_policy_digest)`. |
| `assay.tool_decision_truth.vectors.v0` | 1 | `policies.<name>.version` | 4/4 | `carriers[].carrier.args_digest`, `carriers[].carrier.decision_identity`, `carriers[].carrier.decision_identity.declared_policy_digest`, `carriers[].carrier.decision_identity.observed_input_digest`, and 16 more | A named policy variant a vector exercises. A version label, not a digest over content: comparable for identity between records sharing a naming scheme, not recomputable from bytes. |

The **other keys** column exists because curating one field must not delete the rest from
view. Without it, moving a row into this table would turn every one of its other
configuration keys from a stated finding into a silent gap, which inverts this page's own
first rule.

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
| `assay.enforcement_decision.v0` | 2 | `consumer_negative_controls[].enforcement_decision.action.target_digest`, `consumer_negative_controls[].manifest_establish`, `records[].enforcement_decision.action.target_digest`, `records[].manifest_establish`, and 2 more |
| `assay.enforcement_health.v0` | 1 | `capability_surface.policy_decisions`, `observation_health.policy_layer` |
| `assay.experiment.evidenceref_recompute_consumer.v0` | 1 | `canonicalization_profiles.cbor-deterministic-v1.digest_encoding`, `canonicalization_profiles.cbor-deterministic-v1.digest_prefix`, `canonicalization_profiles.jcs-json-v1.digest_encoding`, `canonicalization_profiles.jcs-json-v1.digest_prefix`, and 5 more |
| `assay.experiment.runner_vs_otel.field_matrix.v0` | 16 | `runner_observation.capability_surface.policy_decisions`, `runner_observation.manifest_digest`, `runner_observation.observation_health.policy_layer`, `summary.manifest_digest_binding`, and 2 more |
| `assay.mcp.tunnel_observed.v0` | 4 | `auth_context.authorization_header_digest`, `data.observed.auth_context.authorization_header_digest`, `data.observed.evidence_refs[].digest`, `data.observed.evidence_refs[].request_envelope_digest`, and 11 more |
| `assay.privileged_mcp_action.verify.report.v0` | 2 | `corpus.manifest`, `descriptor_schema`, `records.required[].constraints.action.target_digest`, `report.source_class_vocabulary` |
| `assay.receipt.openfeature.evaluation_details.v1` | 1 | `events[].data.reducer_version`, `events[].data.source_artifact_digest`, `manifest`, `manifest.producer.version` |
| `assay.runner.capability_diff.v0` | 1 | `policy_outcomes`, `surface.policy_decisions`, `unbound.policy_decisions` |
| `assay.runner.capability_surface.v0` | 18 | `capability_surface.policy_decisions`, `observation_health.policy_layer`, `policy_decisions` |
| `assay.runner.correlation_report.v0` | 1 | `bindings[].policy_decision` |
| `assay.runner.cross_runtime_diff.v0` | 1 | `canonicalization.policy_decisions`, `policy_outcomes`, `sdk_metadata.base.sdk_version`, `sdk_metadata.head.sdk_version`, and 2 more |
| `protectmcp:decision` | 3 | `payload.policy_digest` |

## Outside this page's scope

The scope rule, stated here rather than left in the generator: a record is in scope when
it carries a configuration-ish key **and** is about a decision — by naming one in its own
type, by carrying a `decision` key, or by a configuration key that names one.

**43 further record types** carry configuration-ish keys and fall outside it.
They are counted here so the denominator is visible: "a new schema cannot go unnoticed"
is only true inside a declared scope, and an undeclared one hides its own misses.

| schema | files |
|---|---|
| `a2a.task.lifecycle.export.v1` | 5 |
| `assay.agent_golden_path.v1` | 3 |
| `assay.conformance.adequacy.results.v0` | 1 |
| `assay.conformance.registry.v1` | 1 |
| `assay.coverage_aware_drift.annotation.v0` | 2 |
| `assay.declared_mcp_manifest.v0` | 7 |
| `assay.denied_call_observation.v0` | 1 |
| `assay.docs.evidence-receipts-in-action.manifest.v1` | 1 |
| `assay.enforcement_health.v1` | 5 |
| `assay.experiment.agent_observability_fidelity.redaction_manifest.v0` | 2 |
| `assay.experiment.evidence_mutation_matrix.v0` | 1 |
| `assay.experiment.evidenceref_recompute_consumer.v0` | 1 |
| `assay.experiment.mcp_tool_evidence_binding.binding_cell.v0` | 6 |
| `assay.experiment.otel_span_event_limit.v0` | 1 |
| `assay.manifest_establish.v0` | 1 |
| `assay.mcp-jsonrpc-id-conformance.provenance.v2` | 1 |
| `assay.mcp_server_inventory.v0` | 1 |
| `assay.observability.claim_class_cell.v0` | 1 |
| `assay.otel_projection.v0` | 1 |
| `assay.privileged_mcp_action.candidate_release.v0` | 1 |
| `assay.product-capabilities.v0` | 1 |
| `assay.provider_audit_record.v0` | 2 |
| `assay.receipt-family-matrix.v1` | 1 |
| `assay.receipt.cyclonedx.mlbom-model-component.v1` | 1 |
| `assay.receipt.promptfoo.assertion-component.v1` | 1 |
| `assay.render_safety_conformance.v0` | 1 |
| `assay.runner.archive_manifest.v0` | 13 |
| `assay.runner.kernel_event.v0` | 1 |
| `assay.runner.observation_health.v0` | 16 |
| `assay.runner.runtime_drift.v0.2` | 4 |
| `assay.runner.sdk_event.v0` | 18 |
| `assay.supply_chain_conformance.input.v0` | 2 |
| `assay.tool_annotation_conformance.v0` | 1 |
| `assay.trust-basis.diff.v1` | 3 |
| `browser-use.agent-history.export.v1` | 5 |
| `corpus-adequacy.manifest.v0` | 2 |
| `langfuse.experiment-item-result.export.v1` | 4 |
| `langgraph.stream.tasks.export.v1` | 4 |
| `livekit.function-tools-executed.export.v1` | 2 |
| `mastra.scorer-result.export.v1` | 4 |
| `openai.agents.trace.export.v1` | 5 |
| `ucp.checkout.lifecycle.export.v1` | 5 |
| `x402.requirement-verification.export.v1` | 5 |

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

**PayloadToolDecision (assay-evidence)** — `args_schema_hash, policy_digest, policy_snapshot_digest, policy_snapshot_digest_alg, policy_snapshot_canonicalization, policy_snapshot_schema, tool_definition_digest, tool_definition_digest_alg, tool_definition_canonicalization, tool_definition_schema, tool_definition_source`

No `.json` or `.ndjson` fixture in the tree populates these, which is why they do not appear in the table above. That absence says nothing about their meaning.

**Instances exist.** `crates/assay-evidence/tests/verify_strict_test.rs` builds the payload with these fields populated and runs it through `verify_single_event`; `crates/assay-evidence/src/types/tests.rs` deserializes the same cluster.

**The semantics are stated**, in prose, outside the corpus this generator reads: `docs/architecture/PLAN-P56A-POLICY-SNAPSHOT-DIGEST-VISIBILITY-2026q2.md` (Status: Implemented) for the `policy_snapshot_*` cluster, `PLAN-P56B-TOOL-DEFINITION-DIGEST-BINDING-2026q2.md` for `tool_definition_*`. Both are Status: Implemented and carry per-field MUSTs. `args_schema_hash` is weaker: the only prose on it is one row of `docs/architecture/evidence-metrics-mapping.md` saying how a metric consumes it, not what is hashed or under what canonicalization. That one **is** close to unstated, and saying so is the point — the three citations do not carry equal weight and the page should not imply they do.

The lesson is this page's own rule turned on itself. Searching for populated JSON fixtures and finding none is evidence about **fixtures**, not about semantics. Reading that absence as a gap is the same mistake as reading a field name as a meaning.

## Rules this map follows

- **Not stated is a finding, not a gap.** A schema with no curated subject is emitted as
  such rather than omitted.
- **A mapping is itself a claim.** Saying two fields are the same fact needs a stated
  relation and a direction.
- **No new vocabulary.** This map references what exists and mints nothing.
