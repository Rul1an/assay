# CLI JSON document identities

This file is the **record** of which `assay.<segments>.vN` identities exist in production source,
and which of them are top-level JSON documents a CLI command emits.

It is **hand-edited**. Nothing generates it — not CI, not a committed script, not a local
"generate once and check in". A pin derived from its producer is not a record of that producer; it
is the producer restating itself, and it cannot fail. `crates/assay-cli/tests/cli_json_identities.rs`
reads this file and never writes it.

A scan can tell you that an identity string exists. It cannot tell you whether that string names a
document, an evidence event, a nested object, an input, or a digest domain. **That judgement is what
this file stores**, and it is why the two blocks below are a partition rather than a list.

Adding an identity to the source fails the test until it is recorded here, in the block that says
what it is. Removing one from the source fails until the row goes. See #2167 for the convention and
#2484 for this guard.

## Convention (decided in #2167)

- The discriminator is `schema`, carrying `assay.<segments>.vN`. `.vN` is the breaking generation.
- A closed schema stays closed; a compatible addition keeps the identity and edits the schema in the
  same commit. Our CI stays green; a third party holding a vendored copy of that schema goes red
  later, unsignalled. That is the accepted cost.
- An unknown identity is fail-closed everywhere. Unknown *fields* are rejected on emit validators,
  AEE readers, receipt validators and fail-closed inputs; third-party stdout readers field-pick.

## Machine-checked: identities that ARE CLI JSON documents

A top-level JSON document written to stdout or to a `--report` / artifact path, carrying this string
as its `schema`.

<!-- machine-checked: cli-documents -->
```text
assay.cli.describe.v0
assay.doctor_report.v0
assay.evidence.schema.list.v1
assay.evidence.schema.show.v1
assay.evidence.schema.validation.v1
assay.experiment.runner_phase_timing.v0
assay.init_report.v0
assay.mcp.execution-record-pairing.report.v0
assay.mcp.execution-record-supersession.report.v0
assay.mcp.tunnel-observed.report.v0
assay.mcp_preflight.v0
assay.monitor.observed_peers.v0
assay.privileged_mcp_action.verify.report.v0
assay.run_report.v1
assay.run_summary.v1
assay.side_effect_verification.v0
assay.skill_supply_chain.verify_report.v0
assay.supply_chain_conformance.v0
assay.tool_decision_truth.verify.report.v0
assay.trust-basis.assert.v1
assay.validate_report.v1
```

## Machine-checked: identities that are NOT CLI JSON documents

Evidence events inside a bundle, nested objects, read-only inputs, projections, and one digest
domain. They are recorded so that the partition is total: an identity that is in neither block is a
failed build, which is what stops a new emitter from arriving unnoticed.

<!-- machine-checked: not-cli-documents -->
```text
assay.aee_run_context.v0
assay.aee_seal_key.v0
assay.aee_trust_set.v0
assay.approval_artifact.structured_meta_jcs.v0
assay.denied_call_observation.v0
assay.denied_call_observation.v1
assay.enforcement_decision.v0
assay.enforcement_health.v0
assay.enforcement_health.v1
assay.fallback_projection.v0
assay.mandate.revoked.v1
assay.mandate.used.v1
assay.manifest_establish.v0
assay.mcp.policy.snapshot.v1
assay.mcp.tool-definition.snapshot.v1
assay.mcp.tunnel_observed.v0
assay.mcp_server_inventory.v0
assay.otel_projection.v0
assay.receipt.cyclonedx.mlbom-model-component.v1
assay.receipt.cyclonedx.mlbom_model_component.v1
assay.receipt.livekit.tool-action.v1
assay.receipt.livekit.tool_action.v1
assay.receipt.mastra.score_event.v1
assay.receipt.openfeature.evaluation_details.v1
assay.receipt.promptfoo.assertion-component.v1
assay.receipt.promptfoo.assertion_component.v1
assay.receipt.pydantic.case_result.v1
assay.render_safety_conformance.v0
assay.skill_supply_chain.v0
assay.supply_chain_conformance.input.v0
assay.tool_args.v0
assay.tool_decision_surface.v0
assay.tool_decision_truth.otel_projection.v0
assay.tool_decision_truth.recipe_row.v0
assay.tool_decision_truth.v0
```

## Machine-checked: documents with no identity at all

These are the rows a source scan structurally cannot produce, because **none of them has a dotted
identity constant** — measured, all twelve. A pin built only from identity strings would satisfy every
other check in the guard while omitting every document #2485 has to migrate. They are therefore
*required* rows: a missing one fails the build even though there is nothing in the source to collect.

Each row is `key | producer path | token`, and the test asserts the producer file exists and still
contains the token, so a row cannot outlive the thing it names.

<!-- machine-checked: unnamed-documents -->
```text
baseline | crates/assay-core/src/baseline/mod.rs | schema_version
baseline_diff | crates/assay-cli/src/cli/commands/baseline.rs | baseline.diff(&candidate)
coverage_report | crates/assay-cli/src/cli/commands/coverage/report.rs | coverage_report_v1
discover_inventory | crates/assay-cli/src/cli/commands/discover.rs | DiscoverFormat::Json
hygiene_report | crates/assay-core/src/baseline/report.rs | schema_version
run_json_extended | crates/assay-cli/src/cli/commands/run_output.rs | write_extended_run_json
run_json_minimal | crates/assay-cli/src/cli/commands/run_output.rs | write_run_json_minimal
sarif | crates/assay-core/src/report/sarif.rs | 2.1.0
session_state_window | crates/assay-cli/src/cli/commands/session_state_window.rs | session_state_window_v1
sim_run_report | crates/assay-cli/src/cli/commands/sim.rs | to_string_pretty
soak_report | crates/assay-cli/src/cli/commands/sim/soak/report.rs | soak-report-v1
trust_basis_generate | crates/assay-cli/src/cli/commands/trust_basis.rs | generate_trust_basis
```

| key | version field | class | note |
|---|---|---|---|
| `baseline` | integer, `load` fails on `!= 1` | (c) | live gate; minting an identity is a #2485 review, not a rename |
| `coverage_report` | string `coverage_report_v1` + `report_version` | (b) | closed schema, validated on emit |
| `hygiene_report` | integer | (c) | second document, not `baseline` |
| `run_json_extended` | none | (d) | **not** `assay.run_report.v1`; may be consumed by `bundle create --from` |
| `run_json_minimal` | none | (d) | early-exit shape; same-or-split identity is a #2485 review |
| `sarif` | SARIF `2.1.0` | (g) | foreign identity (OASIS); do not mint an `assay.*` name |
| `session_state_window` | string `session_state_window_v1` + `report_version` | (b) | closed schema, validated on emit |
| `sim_run_report` | none | (d) | `assay sim run --report`; **not** the soak report |
| `baseline_diff` | none | (d) | `assay baseline check --format json` prints `BaselineDiff` |
| `discover_inventory` | none | (d) | `assay mcp discover --format json`; **not** `assay.mcp_server_inventory.v0` |
| `trust_basis_generate` | none | (d) | `assay trust-basis generate` writes canonical JSON; the `assert` report is a separate, named document |
| `soak_report` | string `soak-report-v1` + `report_version` | (b) | closed schema, validated on emit |

Three of these rows — `baseline_diff`, `discover_inventory` and `trust_basis_generate` — came from
an independent read, not from the author of this file. Two separately written inventories both
missed them. That is the argument for the required-rows check stated as evidence rather than as
principle: the documents a scan cannot see are also the ones a single careful reader does not think
of, and there is no instrument that fixes that.

## Confidence

Rows I opened the producer for: every entry in *documents with no identity*, plus `run_report`,
`run_summary`, `validate_report`, `doctor_report`, `describe`, `skill_supply_chain.verify_report`,
`fallback_projection`, and the six receipt families.

Rows classified from the declaration site without following the emit path: the remaining
`not-cli-documents` entries, and `assay.experiment.runner_phase_timing.v0`,
`assay.supply_chain_conformance.v0`, `assay.side_effect_verification.v0`,
`assay.mcp.execution-record-*.report.v0` and `assay.trust-basis.assert.v1` under documents. Those are
the rows most likely to be wrong, and they are named here rather than left for a reader to discover.

`assay.trust-basis.diff.v1` is declared in `assay-evidence` and printed by the CLI. It is out of the
collector's crates and therefore **not** covered by this guard today. That is a known hole, recorded
rather than silently omitted.

## What is not here

Bundle manifests, trust cards, attestation and lint packs keep their own integers under #2552. The
CLI field-name rule does not cross that crate boundary by implication.
