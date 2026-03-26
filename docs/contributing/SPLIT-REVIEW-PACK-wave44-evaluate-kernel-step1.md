# Wave44 Evaluate Kernel Step1 Review Pack (Freeze)

## Intent

Freeze Wave44 scope for `crates/assay-core/src/mcp/tool_call_handler/evaluate.rs` before any mechanical moves.

## Scope

- `docs/contributing/SPLIT-PLAN-wave44-evaluate-kernel.md`
- `docs/contributing/SPLIT-CHECKLIST-wave44-evaluate-kernel-step1.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave44-evaluate-kernel-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave44-evaluate-kernel-step1.md`
- `scripts/ci/review-wave44-evaluate-kernel-step1.sh`

## Non-goals

- no changes under `crates/assay-core/src/mcp/tool_call_handler/**`
- no changes under `crates/assay-core/tests/**`
- no workflow changes
- no payload-shape drift
- no reason-code drift
- no obligation-normalization drift

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-wave44-evaluate-kernel-step1.sh
```

Gate includes:

```bash
cargo fmt --check
cargo clippy -p assay-core --all-targets -- -D warnings
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::approval_required_missing_denies' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::restrict_scope_target_missing_denies' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::redact_args_target_missing_denies' -- --exact
cargo test -q -p assay-core --test tool_taxonomy_policy_match tool_taxonomy_policy_match_handler_decision_event_records_classes -- --exact
cargo test -q -p assay-core --test fulfillment_normalization fulfillment_normalizes_outcomes_and_sets_policy_deny_path -- --exact
cargo test -q -p assay-core --test replay_diff_contract classify_replay_diff_unchanged -- --exact
```

## Reviewer 60s scan

1. Confirm diff is only the 5 Step1 files.
2. Confirm workflow-ban and source/test subtree bans exist in the script.
3. Confirm targeted tests are pinned with `--exact`.
4. Confirm Step2 preview is module-cut only, not redesign.
5. Run reviewer script and expect PASS.
