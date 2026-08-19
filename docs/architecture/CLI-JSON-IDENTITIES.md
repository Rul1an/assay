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

A top-level JSON document a command writes to stdout or to a caller-named path, carrying this
string as its `schema`.

Each row is `identity | writing file | naming file`, where `-` means the writing file also names the
identity. The test asserts three things: the writing file calls a writer, the naming file carries the
identity, and **the writing file is about this identity** — it names the identity itself, or, when
the two files differ, mentions a symbol the naming file publishes. Without the third, a row could be
bound to a file that writes some other document and happens to call a writer.
The third column exists because some documents split the two: `assay doctor` sets its `schema` in
`diagnostics/probes.rs` and writes in `doctor/implementation.rs`, and the three `evidence schema`
reports are declared in `schema/reports.rs` and written in `schema/write.rs`. Recording the split is
better than loosening the check until it fits. Membership alone was not enough: an earlier
revision of this file classified six of these as non-documents on the strength of a word in their
name — "carrier", "projection", "health artifact" — and the partition stayed green, because it only
checked that every identity appeared somewhere. Two independent reviews found all six. Binding the
row to a write is what turns that class of mistake into a failure.

<!-- machine-checked: cli-documents -->
```text
assay.cli.describe.v0 | crates/assay-cli/src/cli/commands/describe.rs | -
assay.doctor_report.v0 | crates/assay-cli/src/cli/commands/doctor/implementation.rs | crates/assay-cli/src/diagnostics/probes.rs
assay.enforcement_health.v0 | crates/assay-cli/src/cli/commands/monitor_next/enforcement_health.rs | -
assay.enforcement_health.v1 | crates/assay-cli/src/enforcement_health_v1.rs | -
assay.evidence.schema.list.v1 | crates/assay-cli/src/cli/commands/evidence/schema/write.rs | crates/assay-cli/src/cli/commands/evidence/schema/reports.rs
assay.evidence.schema.show.v1 | crates/assay-cli/src/cli/commands/evidence/schema/write.rs | crates/assay-cli/src/cli/commands/evidence/schema/reports.rs
assay.evidence.schema.validation.v1 | crates/assay-cli/src/cli/commands/evidence/schema/write.rs | crates/assay-cli/src/cli/commands/evidence/schema/reports.rs
assay.experiment.runner_phase_timing.v0 | crates/assay-cli/src/cli/commands/runner_spike/phases.rs | -
assay.init_report.v0 | crates/assay-cli/src/cli/commands/init_report.rs | -
assay.mcp.execution-record-pairing.report.v0 | crates/assay-cli/src/cli/commands/evidence/mcp_execution_records.rs | -
assay.mcp.execution-record-supersession.report.v0 | crates/assay-cli/src/cli/commands/evidence/mcp_supersession.rs | -
assay.mcp.tunnel-observed.report.v0 | crates/assay-cli/src/cli/commands/evidence/mcp_tunnel_observed.rs | -
assay.mcp_preflight.v0 | crates/assay-cli/src/cli/commands/mcp/preflight.rs | -
assay.mcp_server_inventory.v0 | crates/assay-cli/src/cli/commands/inventory.rs | crates/assay-core/src/discovery/inventory_carrier.rs
assay.monitor.observed_peers.v0 | crates/assay-cli/src/cli/commands/monitor_next/observed_peers.rs | -
assay.otel_projection.v0 | crates/assay-cli/src/cli/commands/project_otel.rs | crates/assay-core/src/otel/projection.rs
assay.runner.observation_health.v0 | crates/assay-cli/src/cli/commands/monitor_next/observation_health.rs | crates/assay-runner-schema/src/health.rs
assay.privileged_mcp_action.verify.report.v0 | crates/assay-cli/src/cli/commands/evidence/verify_privileged_mcp_action.rs | -
assay.run_report.v1 | crates/assay-core/src/report/json.rs | -
assay.run_summary.v1 | crates/assay-core/src/report/summary/writer.rs | -
assay.side_effect_verification.v0 | crates/assay-cli/src/cli/commands/evidence/verify_side_effects.rs | -
assay.skill_supply_chain.v0 | crates/assay-cli/src/cli/commands/evidence/skill_supply_chain_capture.rs | -
assay.skill_supply_chain.verify_report.v0 | crates/assay-cli/src/cli/commands/evidence/verify_skill_supply_chain.rs | -
assay.supply_chain_conformance.v0 | crates/assay-cli/src/cli/commands/supply_chain_conformance.rs | crates/assay-registry/src/supply_chain.rs
assay.tool_decision_truth.otel_projection.v0 | crates/assay-cli/src/cli/commands/project_otel.rs | crates/assay-core/src/otel/projection.rs
assay.tool_decision_truth.verify.report.v0 | crates/assay-cli/src/cli/commands/evidence/verify_tool_decision_truth.rs | -
assay.trust-basis.assert.v1 | crates/assay-cli/src/cli/commands/trust_basis.rs | -
assay.trust-basis.diff.v1 | crates/assay-cli/src/cli/commands/trust_basis.rs | crates/assay-evidence/src/trust_basis/types.rs
assay.validate_report.v1 | crates/assay-cli/src/cli/commands/validate.rs | -
```

`assay.trust-basis.diff.v1` is declared in `assay-evidence`, outside the scanned crates. An earlier
revision recorded that as a known hole. It is not a hole: the CLI writes the document
(`write_diff_json`), and following the write rather than the crate puts it here with everything else.

## Machine-checked: identities that are NOT CLI JSON documents

Evidence events inside a bundle, nested objects, read-only inputs, and one digest domain. Each row is
`identity | reason`, and the reason is required.

The partition is total, so an identity in neither block fails the build. That stops a new emitter
arriving unnoticed. It does not stop a *misclassification*, and no static check can: naming and
writing live in different files for several of these, so nothing can decide from source alone that
`assay.otel_projection.v0` is the document `assay project-otel` writes. Requiring a reason is what
this file can do about it — moving a real document into this block now means writing a sentence that
is false, rather than deleting a line. Two independent reviews caught six such moves in the previous
revision, when this block was a bare list.

<!-- machine-checked: not-cli-documents -->
```text
assay.coding_agent.evidence_pack.v0 | declared in assay-evidence as a bundle pack schema; no CLI write opened
assay.mandate.v1 | mandate event carried in evidence, not written by a command
assay.mcp_manifest_observed.v0 | assay-mcp-server observation event inside a bundle
assay.mcp_manifest_projection.v0 | assay-mcp-server projection event inside a bundle
assay.mcp_tool_field.v0 | a field-level record nested in the manifest observation
assay.provider_audit_record.v0 | assay-mcp-server audit record written to the audit log, not a command document
assay.redaction_receipt.v0 | runner receipt inside an archive
assay.runner.archive_manifest.v0 | runner archive manifest; #2552 owns archive integers and identities
assay.runner.capability_surface.v0 | runner capability surface read by project-otel as input
assay.runner.claim_support_parity.v0 | runner parity artifact inside an archive
assay.runner.correlation_report.v0 | runner correlation artifact inside an archive
assay.runner.coverage_descriptor.v0 | runner coverage descriptor inside an archive
assay.runner.event.v0 | runner event stream entry
assay.runner.fidelity_verdict.v0 | runner verdict inside an archive
assay.runner.kernel_event.v0 | kernel event stream entry
assay.runner.path_projection.v0 | runner path projection inside an archive
assay.runner.policy_event.v0 | policy event stream entry
assay.runner.sdk_event.v0 | SDK event stream entry
assay.semantic-digest.jcs-rfc8785.v1 | a canonicalization profile name, not a document
assay.token_passthrough_conformance.v0 | assay-mcp-server conformance carrier inside a bundle
assay.tool_annotation_conformance.v0 | assay-mcp-server conformance carrier inside a bundle
assay.aee_run_context.v0 | input the sandbox reads; exact key set enforced on read
assay.aee_seal_key.v0 | input the sandbox reads; exact key set enforced on read
assay.aee_trust_set.v0 | input the sandbox reads; exact key set enforced on read
assay.approval_artifact.structured_meta_jcs.v0 | a canonicalization domain for approval metadata, not a document
assay.denied_call_observation.v0 | nested inside the privileged-mcp-action verify report
assay.denied_call_observation.v1 | nested inside the privileged-mcp-action verify report
assay.enforcement_decision.v0 | evidence event read from a bundle; the command writes a verify report about it
assay.fallback_projection.v0 | an input mode name for mcp-execution-records, not a schema it emits
assay.mandate.revoked.v1 | MCP lifecycle event type inside a bundle
assay.mandate.used.v1 | MCP lifecycle event type inside a bundle
assay.manifest_establish.v0 | nested inside the privileged-mcp-action verify report
assay.mcp.policy.snapshot.v1 | policy snapshot event carried in evidence, not written by a command
assay.mcp.tool-definition.snapshot.v1 | tool-definition snapshot event carried in evidence
assay.mcp.tunnel_observed.v0 | the carrier event; the command writes assay.mcp.tunnel-observed.report.v0
assay.receipt.cyclonedx.mlbom-model-component.v1 | receipt written into an evidence bundle by BundleWriter, never to stdout
assay.receipt.cyclonedx.mlbom_model_component.v1 | event type beside that receipt schema, same bundle write
assay.receipt.livekit.tool-action.v1 | receipt written into an evidence bundle by BundleWriter, never to stdout
assay.receipt.livekit.tool_action.v1 | event type beside that receipt schema, same bundle write
assay.receipt.mastra.score_event.v1 | receipt written into an evidence bundle by BundleWriter, never to stdout
assay.receipt.openfeature.evaluation_details.v1 | receipt written into an evidence bundle by BundleWriter, never to stdout
assay.receipt.promptfoo.assertion-component.v1 | receipt written into an evidence bundle by BundleWriter, never to stdout
assay.receipt.promptfoo.assertion_component.v1 | event type beside that receipt schema, same bundle write
assay.receipt.pydantic.case_result.v1 | receipt written into an evidence bundle by BundleWriter, never to stdout
assay.render_safety_conformance.v0 | a golden corpus identity compared against, not a document a command mints
assay.supply_chain_conformance.input.v0 | the input descriptor the command reads; it writes assay.supply_chain_conformance.v0
assay.tool_args.v0 | a digest domain string; nothing serializes it as a document
assay.tool_decision_surface.v0 | event type read from a bundle by verify-side-effects
assay.tool_decision_truth.recipe_row.v0 | a row inside the projection, not the projection
assay.tool_decision_truth.v0 | `assay mcp --tool-decision-truth-out` DOES append these to a caller-named path, one JSON object per line; the file is NDJSON and no line is the top-level document. An earlier reason here said "read as input", which was the same keyword error as "projection"
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

## What this guard cannot do, stated plainly

- **It cannot verify a classification, and the reason column is a speed bump, not a gate.** The test
  checks that a row has a `|` and a non-empty right-hand side. It does not read the sentence: `a
  banana` passes. A misclassification costs writing a false sentence instead of deleting a line,
  which is why two independent reviews caught six of them — but the build stays green either way.
  The theory that produced those six ("a projection, not a document") writes a perfectly non-empty
  reason. **#2485 must not read these sentences as evidence that a row is right.** Three of them have
  been wrong today, and the third was written after conceding the first two.
- **The writer-to-identity tie is a token check, not dataflow.** A writer that mentions the right
  constant while serializing something else would pass. Closing that means following the value into
  the `schema` field, which nothing here does.

- **The writer list is a fixed set of idioms**, and it has been wrong twice. `serde_json::to_writer`
  was missing until `assay describe` exposed it; `to_vec_pretty`, `tokio::fs::write`,
  `serde_json::to_string` and a bare `fs::write` were missing until a review opened the emit paths.
  A file emitting through some route still not on the list is invisible to
  `documents_are_bound_to_a_writer`.

- **The guard follows rows to writers, never writers to rows.** A document row must name a file that
  writes; a file that writes need not be named by any row. That is the hole
  `assay.runner.observation_health.v0` fell through. Measured on this head: **31 production files
  under `cli/commands` serialize JSON and are named by no row.** Some of them almost certainly emit
  documents — `evidence lint`, `evidence diff`, `evidence list`, `store-status`, `explain`, `import`,
  `trace`. Closing this means requiring every JSON-serializing command file to be named by a row or
  to carry an explicit opt-out, and triaging those 31. It is tracked separately rather than answered
  here, because thirty-one classifications written in one sitting is the failure this file exists to
  record.
- **Twenty-one of the non-document rows are declared in dependency crates and their emit paths were
  not opened.** They are classified from the declaring module. That is a weaker basis than the rows
  whose producer I read, and three such judgements have already been wrong today.

- **Five crates declare identities the CLI writes** — `assay-cli`, `assay-core`, `assay-evidence`
  (`trust-basis.diff`), `assay-registry` (`supply_chain_conformance`) and `assay-runner-schema`
  (`runner.observation_health`). The collector scans the first two; the rest are reached only through
  a document row's naming column, which is why `runner.observation_health` was missing from the first
  revision of this file and the guard was green without it. A **sixth** crate is now caught:
  `every_production_identity_is_classified` also collects `pub const` identities from every crate
  `assay-cli` depends on, and `DEPENDENCY_CRATES` is pinned against `assay-cli/Cargo.toml` so the
  list cannot drift away from the manifest. What that rule does *not* catch is a new command file in
  a crate already scanned — `evidence lint` and `evidence diff` are that class, and it is #2555.
- **`describe ⊆ pin` is one-directional.** `BINDING_ROWS` is seven clap paths and omits the run
  report and run summary. Forcing `describe` to grow is a product decision, not bookkeeping.

## Provenance of this file

The identity strings were located by searching source; the classification, the writer bindings and
every reason are hand-made judgements, and they are the content. Three rows — `baseline_diff`,
`discover_inventory`, `trust_basis_generate` — came from an independent read after two separately
written inventories missed them. Six identities were misclassified as non-documents in the first
revision on the strength of a word in their name ("carrier", "projection", "health artifact"); two
independent reviews found all six, which is why the reason column exists.

## What is not here

Bundle manifests, trust cards, attestation and lint packs keep their own integers under #2552. The
CLI field-name rule does not cross that crate boundary by implication.
