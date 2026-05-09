# SPLIT PLAN - Wave 52 LiveKit Tool Action Importer

## Goal

Reduce `crates/assay-cli/src/cli/commands/evidence/livekit_tool_action.rs` behind a stable CLI/importer facade without changing LiveKit acted-family receipt semantics.

Current hotspot baseline on `origin/main @ 057151c5`:

- `crates/assay-cli/src/cli/commands/evidence/livekit_tool_action.rs`: `1104` LOC
- public CLI entrypoint: `cmd_livekit_tool_action(LiveKitToolActionArgs)`
- companion integration test: `crates/assay-cli/tests/evidence_test.rs::test_livekit_imported_tool_action_receipts_verify_and_do_not_mutate_trust_basis_claims`

## Why This Is Next

Wave51 retired the runner, sandbox, MCP proxy, and Trust Basis hotspots. A fresh `origin/main` LOC snapshot now shows the LiveKit tool-action importer as the largest handwritten production Rust file. It is also a fresh protocol-facing importer, so the safe next move is a freeze-only step before any module extraction.

## Frozen Behavior

Wave52 freezes these importer contracts before moving code:

- CLI arguments and command routing for `evidence import livekit-tool-action`
- event type/source/schema constants
- accepted reduced input shape and required/optional top-level keys
- forbidden raw payload/session/trace/telemetry keys
- call/output pairing by `call_id` before list order
- JSONL multi-document input behavior
- bounded reviewer-safe string validation
- raw argument/output hashing without raw value leakage
- timestamp normalization and deterministic import-time handling
- malformed missing-output rejection
- non-integer raw float rejection
- Trust Basis non-mutation for LiveKit acted-family receipts

## Step1 - Freeze and Gate

Branch: `codex/wave52-livekit-tool-action-freeze-step1` (base: `main`)

Step1 is docs+gate only. It must not edit production importer code or tests.

Deliverables:

- `docs/contributing/SPLIT-PLAN-wave52-livekit-tool-action.md`
- `docs/contributing/SPLIT-CHECKLIST-wave52-livekit-tool-action-step1.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave52-livekit-tool-action-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave52-livekit-tool-action-step1.md`
- `scripts/ci/review-wave52-livekit-tool-action-step1.sh`
- `docs/contributing/SPLIT-PLAN-week8-sota-gates-2026q2.md`
- `docs/security/OWASP-MCP-TOP10-TEST-MAP.md`
- `scripts/ci/optional-public-api-drift.sh`
- `scripts/ci/mutation-smoke-pure-modules.sh`
- `scripts/ci/review-week8-sota-gates.sh`

## Step2 - Mechanical Split Preview

Only after Step1 lands, split behind the existing command facade.

Suggested target layout:

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
- no hash/canonical JSON drift
- no timestamp or source-artifact-ref drift
- no trust-basis classifier drift
- preserve existing tests, moving them only if the split needs it

## Stop Rule

If Step2 reduces the facade below roughly 150 LOC and keeps protocol helpers isolated, stop. Do not keep splitting a fresh importer without concrete review pain.
