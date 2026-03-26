# Wave45 Plan — `mcp/policy/engine.rs` Kernel Split

## Goal

Split `crates/assay-core/src/mcp/policy/engine.rs` behind a stable facade with zero policy
semantic drift and no downstream decision/event contract drift.

Current hotspot baseline on `origin/main @ 7709a25f`:
- `crates/assay-core/src/mcp/policy/engine.rs`: `799` LOC
- `crates/assay-core/tests/policy_engine_test.rs`: `128` LOC
- `crates/assay-core/tests/tool_taxonomy_policy_match.rs`: `118` LOC
- `crates/assay-core/tests/decision_emit_invariant.rs`: `1293` LOC

## Step1 (freeze)

Branch: `codex/wave45-policy-engine-step1` (base: `main`)

Deliverables:
- `docs/contributing/SPLIT-PLAN-wave45-policy-engine.md`
- `docs/contributing/SPLIT-CHECKLIST-wave45-policy-engine-step1.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave45-policy-engine-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave45-policy-engine-step1.md`
- `scripts/ci/review-wave45-policy-engine-step1.sh`

Step1 constraints:
- docs+gate only
- no edits under `crates/assay-core/src/mcp/policy/**`
- no edits under `crates/assay-core/tests/**`
- no workflow edits
- no handler / decision / evidence / CLI edits

Step1 gate:
- allowlist-only diff (the 5 Step1 files)
- workflow-ban (`.github/workflows/*`)
- hard fail on tracked changes in `crates/assay-core/src/mcp/policy/**`
- hard fail on untracked files in `crates/assay-core/src/mcp/policy/**`
- hard fail on tracked changes in `crates/assay-core/tests/**`
- hard fail on untracked files in `crates/assay-core/tests/**`
- `cargo fmt --check`
- `cargo clippy -p assay-core --all-targets -- -D warnings`
- targeted tests:
  - `cargo test -q -p assay-core --test policy_engine_test test_mixed_tools_config -- --exact`
  - `cargo test -q -p assay-core --test policy_engine_test test_constraint_enforcement -- --exact`
  - `cargo test -q -p assay-core --test tool_taxonomy_policy_match tool_taxonomy_policy_match_policy_file_blocks_alt_sink_by_class -- --exact`
  - `cargo test -q -p assay-core --test tool_taxonomy_policy_match tool_taxonomy_policy_match_handler_decision_event_records_classes -- --exact`
  - `cargo test -q -p assay-core --test decision_emit_invariant approval_required_missing_denies -- --exact`
  - `cargo test -q -p assay-core --test decision_emit_invariant restrict_scope_target_missing_denies -- --exact`
  - `cargo test -q -p assay-core --test decision_emit_invariant redact_args_target_missing_denies -- --exact`
  - `cargo test -q -p assay-core --lib 'mcp::policy::engine::tests::parse_delegation_context_uses_explicit_depth_only' -- --exact`

## Frozen public surface

Wave45 freezes the expectation that Step2 keeps these stable policy entrypoints and consumer-facing
contracts unchanged in meaning:
- `McpPolicy::evaluate`
- `McpPolicy::evaluate_with_metadata`
- `McpPolicy::check`
- `PolicyEvaluation`
- `PolicyDecision`
- `PolicyMatchMetadata`
- `PolicyObligation`

Step2 may reorganize internal ownership behind `engine.rs`, but must not redefine:
- allow/deny outcomes
- precedence / specificity handling
- default / fail-closed behavior
- reason-code or policy-code strings
- metadata fields projected into downstream decision events

## Step2 (mechanical split preview)

Branch: `codex/wave45-policy-engine-step2` (base: `main`)

Target layout:
- `crates/assay-core/src/mcp/policy/engine.rs` (thin facade + stable routing)
- `crates/assay-core/src/mcp/policy/engine_next/mod.rs`
- `crates/assay-core/src/mcp/policy/engine_next/matcher.rs`
- `crates/assay-core/src/mcp/policy/engine_next/effects.rs`
- `crates/assay-core/src/mcp/policy/engine_next/precedence.rs`
- `crates/assay-core/src/mcp/policy/engine_next/fail_closed.rs`
- `crates/assay-core/src/mcp/policy/engine_next/diagnostics.rs`

Step2 scope:
- `crates/assay-core/src/mcp/policy/engine.rs`
- `crates/assay-core/src/mcp/policy/engine_next/mod.rs`
- `crates/assay-core/src/mcp/policy/engine_next/matcher.rs`
- `crates/assay-core/src/mcp/policy/engine_next/effects.rs`
- `crates/assay-core/src/mcp/policy/engine_next/precedence.rs`
- `crates/assay-core/src/mcp/policy/engine_next/fail_closed.rs`
- `crates/assay-core/src/mcp/policy/engine_next/diagnostics.rs`
- `docs/contributing/SPLIT-PLAN-wave45-policy-engine.md`
- `docs/contributing/SPLIT-CHECKLIST-wave45-policy-engine-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave45-policy-engine-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave45-policy-engine-step2.md`
- `scripts/ci/review-wave45-policy-engine-step2.sh`

Step2 principles:
- 1:1 body moves
- stable `McpPolicy::{evaluate,evaluate_with_metadata,check}` behavior
- no allow/deny drift
- no precedence or specificity drift
- no default / fail-closed drift
- no reason-code renames
- no metadata projection drift into downstream decision events
- no edits under `crates/assay-core/tests/**`
- no workflow edits

Current Step2 shape:
- `engine.rs`: facade target `<= 320` LOC
- `engine_next/matcher.rs`: tool/class match helpers
- `engine_next/effects.rs`: obligation capture and contract evaluation helpers
- `engine_next/precedence.rs`: deny/allow precedence helpers
- `engine_next/fail_closed.rs`: tool-drift, rate-limit, schema-deny, and unconstrained fallback helpers
- `engine_next/diagnostics.rs`: metadata finalization and delegation parsing helpers

## Step3 (closure)

Docs+gate-only closure slice that re-runs Step2 invariants and limits any follow-up to
micro-cleanup only.

## Promote

Stacked chain:
- Step1 -> `main`
- Step2 -> Step1
- Step3 -> Step2

Final promote PR to `main` from Step3 once the chain is clean.

## Reviewer notes

This wave must remain policy-engine split planning only.

Primary failure modes:
- sneaking semantic cleanup into a mechanical split
- changing precedence or fail-closed behavior while chasing file size
- renaming reason/policy codes under a refactor label
- leaking scope into handler, decision, evidence, or CLI surfaces
