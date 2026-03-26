# Wave44 Plan — `mcp/tool_call_handler/evaluate.rs` Kernel Split

## Goal

Split `crates/assay-core/src/mcp/tool_call_handler/evaluate.rs` behind a stable facade with zero behavior change and no emitted contract drift.

Current hotspot baseline on `origin/main @ aa10d921`:
- `crates/assay-core/src/mcp/tool_call_handler/evaluate.rs`: `1016` LOC
- `crates/assay-core/src/mcp/tool_call_handler/tests.rs`: `1242` LOC
- `crates/assay-core/tests/decision_emit_invariant.rs`: `1293` LOC
- `crates/assay-core/tests/fulfillment_normalization.rs`: `165` LOC
- `crates/assay-core/tests/replay_diff_contract.rs`: `382` LOC
- `crates/assay-core/tests/tool_taxonomy_policy_match.rs`: `133` LOC

## Step1 (freeze)

Branch: `codex/wave44-evaluate-kernel-step1` (base: `main`)

Deliverables:
- `docs/contributing/SPLIT-PLAN-wave44-evaluate-kernel.md`
- `docs/contributing/SPLIT-CHECKLIST-wave44-evaluate-kernel-step1.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave44-evaluate-kernel-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave44-evaluate-kernel-step1.md`
- `scripts/ci/review-wave44-evaluate-kernel-step1.sh`

Step1 constraints:
- docs+gate only
- no edits under `crates/assay-core/src/mcp/tool_call_handler/**`
- no edits under `crates/assay-core/tests/**`
- no workflow edits

Step1 gate:
- allowlist-only diff (the 5 Step1 files)
- workflow-ban (`.github/workflows/*`)
- hard fail on tracked changes in `crates/assay-core/src/mcp/tool_call_handler/**`
- hard fail on untracked files in `crates/assay-core/src/mcp/tool_call_handler/**`
- hard fail on tracked changes in `crates/assay-core/tests/**`
- hard fail on untracked files in `crates/assay-core/tests/**`
- `cargo fmt --check`
- `cargo clippy -p assay-core --all-targets -- -D warnings`
- targeted tests:
  - `cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::approval_required_missing_denies' -- --exact`
  - `cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::restrict_scope_target_missing_denies' -- --exact`
  - `cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::redact_args_target_missing_denies' -- --exact`
  - `cargo test -q -p assay-core --test tool_taxonomy_policy_match tool_taxonomy_policy_match_handler_decision_event_records_classes -- --exact`
  - `cargo test -q -p assay-core --test fulfillment_normalization fulfillment_normalizes_outcomes_and_sets_policy_deny_path -- --exact`
  - `cargo test -q -p assay-core --test replay_diff_contract classify_replay_diff_unchanged -- --exact`

## Step2 (mechanical split preview)

Branch: `codex/wave44-evaluate-kernel-step2` (base: `main`)

Target layout:
- `crates/assay-core/src/mcp/tool_call_handler/evaluate.rs` (thin facade + `handle_tool_call`)
- `crates/assay-core/src/mcp/tool_call_handler/evaluate_next/mod.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate_next/approval.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate_next/scope.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate_next/redaction.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate_next/fail_closed.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate_next/classification.rs`

Step2 scope:
- `crates/assay-core/src/mcp/tool_call_handler/mod.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate_next/mod.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate_next/approval.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate_next/scope.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate_next/redaction.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate_next/fail_closed.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate_next/classification.rs`
- `docs/contributing/SPLIT-PLAN-wave44-evaluate-kernel.md`
- `docs/contributing/SPLIT-CHECKLIST-wave44-evaluate-kernel-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave44-evaluate-kernel-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave44-evaluate-kernel-step2.md`
- `scripts/ci/review-wave44-evaluate-kernel-step2.sh`

Step2 principles:
- 1:1 body moves
- stable `handle_tool_call` behavior and same deny/allow routing
- no changes to emitted payload shape or additive field presence
- no reason-code renames
- no obligation outcome normalization drift
- no request-id / `tool_call_id` extraction drift
- no mandate/authz semantic changes
- no edits under `crates/assay-core/tests/**`
- no workflow edits

Current Step2 shape:
- `evaluate.rs`: `266` LOC
- `evaluate_next/approval.rs`: `177` LOC
- `evaluate_next/scope.rs`: `126` LOC
- `evaluate_next/redaction.rs`: `232` LOC
- `evaluate_next/fail_closed.rs`: `55` LOC
- `evaluate_next/classification.rs`: `191` LOC

## Step3 (closure)

Branch: `codex/wave44-evaluate-kernel-step3` (base: `main`)

Deliverables:
- `docs/contributing/SPLIT-PLAN-wave44-evaluate-kernel.md`
- `docs/contributing/SPLIT-CHECKLIST-wave44-evaluate-kernel-step3.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave44-evaluate-kernel-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave44-evaluate-kernel-step3.md`
- `scripts/ci/review-wave44-evaluate-kernel-step3.sh`

Step3 constraints:
- docs+gate only
- no edits under `crates/assay-core/src/mcp/tool_call_handler/**`
- no edits under `crates/assay-core/tests/**`
- no workflow edits
- no new module cuts
- no behavior cleanup beyond internal follow-up notes

Step3 gate:
- allowlist-only diff (the 5 Step3 files)
- workflow-ban (`.github/workflows/*`)
- hard fail on tracked changes in `crates/assay-core/src/mcp/tool_call_handler/**`
- hard fail on untracked files in `crates/assay-core/src/mcp/tool_call_handler/**`
- hard fail on tracked changes in `crates/assay-core/tests/**`
- hard fail on untracked files in `crates/assay-core/tests/**`
- `cargo fmt --check`
- `cargo clippy -p assay-core --all-targets -- -D warnings`
- targeted tests:
  - `cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::approval_required_missing_denies' -- --exact`
  - `cargo test -q -p assay-core --test tool_taxonomy_policy_match tool_taxonomy_policy_match_handler_decision_event_records_classes -- --exact`
  - `cargo test -q -p assay-core --test fulfillment_normalization fulfillment_normalizes_outcomes_and_sets_policy_deny_path -- --exact`
  - `cargo test -q -p assay-core --test replay_diff_contract classify_replay_diff_unchanged -- --exact`

## Promote

Stacked chain:
- Step1 -> `main`
- Step2 -> Step1
- Step3 -> Step2

Final promote PR to `main` from Step3 once the chain is clean.

## Shipped status

Wave44 Step2 shipped on `main` via `#958`.

Wave44 Step3 is intentionally smaller:
- close the split with docs/gates that forbid redesign drift
- keep `evaluate.rs` as the stable facade entrypoint
- keep `evaluate_next/*` as the split implementation ownership boundary
- bound any follow-up to micro-cleanup only

## Reviewer notes

This wave must remain evaluate-kernel closure only.

Primary failure modes:
- sneaking new `tool_call_handler/**` edits into a closure slice
- expanding Step3 into another code refactor
- loosening deny/fulfillment/replay invariants after Step2 shipped
- proposing new module cuts before the split has had time to harden on `main`
