# SPLIT PLAN - Wave 52 LiveKit Tool Action Importer

## Goal

Reduce `crates/assay-cli/src/cli/commands/evidence/livekit_tool_action.rs`
behind a stable CLI/importer facade without changing LiveKit acted-family
receipt semantics.

Current hotspot baseline on `origin/main @ 057151c5`:

- `crates/assay-cli/src/cli/commands/evidence/livekit_tool_action.rs`: `1104` LOC
- public CLI entrypoint: `cmd_livekit_tool_action(LiveKitToolActionArgs)`
- companion integration test:
  `crates/assay-cli/tests/evidence_test.rs::test_livekit_imported_tool_action_receipts_verify_and_do_not_mutate_trust_basis_claims`

## Why This Is Next

After Wave51, the LiveKit tool-action importer became the largest handwritten
production Rust file in `assay-cli`. It is also a fresh protocol-facing
surface, so the safe next move is not a blind module split. First align the
behavior with upstream LiveKit feedback, then freeze it, then split
mechanically.

## Frozen Behavior After Alignment

Wave52 freezes these importer contracts before moving code:

- CLI arguments and command routing for `evidence import livekit-tool-action`
- event type/source/schema constants
- accepted reduced input shape and required/optional top-level keys
- forbidden raw payload/session/trace/telemetry keys
- call/output pairing by LiveKit SDK list order
- optional `call_id` consistency checking when every paired entry has one
- partial `call_id` presence remains list-order paired
- `FunctionCallOutput | None` is preserved as `completed=false`
- JSONL multi-document input behavior
- bounded reviewer-safe string validation
- raw argument/output hashing without raw value leakage
- timestamp normalization and deterministic import-time handling
- non-integer raw float rejection
- Trust Basis non-mutation for LiveKit acted-family receipts

## Step1 - Upstream Alignment And Freeze

Step1 aligns the importer with LiveKit's source-level
`FunctionToolsExecutedEvent` semantics and keeps module extraction out of
scope.

Deliverables:

- importer list-order pairing alignment
- null-output receipt behavior
- fixture, schema, and docs updates
- Wave52 split plan/checklist/move map/review pack
- reviewer gate for the aligned surface
- Week 8 SOTA gate plan and optional scripts
- OWASP MCP Top 10 test coverage map

## Step2 - Mechanical Split

Only after Step1 lands, split behind the existing command facade. Suggested
target layout:

```text
crates/assay-cli/src/cli/commands/evidence/livekit_tool_action.rs
crates/assay-cli/src/cli/commands/evidence/livekit_tool_action/
  input.rs
  reduce.rs
  validate.rs
  canonical.rs
  bundle.rs
  tests.rs
```

Step2 principles:

- keep `cmd_livekit_tool_action` and `LiveKitToolActionArgs` in the facade
- move function bodies 1:1 where possible
- no schema/key allowlist drift
- no pairing, null-output, hash, or canonical JSON drift
- no timestamp or source-artifact-ref drift
- no trust-basis classifier drift
- preserve existing tests, moving them only if the split needs it

## Stop Rule

If Step2 reduces the facade below roughly 150 LOC and keeps protocol helpers
isolated, stop. Do not keep splitting a fresh importer without concrete review
pain.
