# Wave44 Evaluate Kernel Step3 Review Pack (Closure)

## Intent

Close the shipped Wave44 evaluate-kernel split with docs/gates only and forbid redesign drift after Step2 landed on `main`.

## Scope

- `docs/contributing/SPLIT-PLAN-wave44-evaluate-kernel.md`
- `docs/contributing/SPLIT-CHECKLIST-wave44-evaluate-kernel-step3.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave44-evaluate-kernel-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave44-evaluate-kernel-step3.md`
- `scripts/ci/review-wave44-evaluate-kernel-step3.sh`

## Non-goals

- no workflow changes
- no changes under `crates/assay-core/src/mcp/tool_call_handler/**`
- no changes under `crates/assay-core/tests/**`
- no new module cuts
- no deny/fulfillment/replay redesign

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-wave44-evaluate-kernel-step3.sh
```

Gate includes:

```bash
cargo fmt --check
cargo clippy -p assay-core --all-targets -- -D warnings
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::approval_required_missing_denies' -- --exact
cargo test -q -p assay-core --test tool_taxonomy_policy_match tool_taxonomy_policy_match_handler_decision_event_records_classes -- --exact
cargo test -q -p assay-core --test fulfillment_normalization fulfillment_normalizes_outcomes_and_sets_policy_deny_path -- --exact
cargo test -q -p assay-core --test replay_diff_contract classify_replay_diff_unchanged -- --exact
```

## Reviewer 60s scan

1. Confirm the diff is limited to the Step3 allowlist.
2. Confirm `tool_call_handler/**` and `crates/assay-core/tests/**` are completely frozen in this wave.
3. Confirm the plan records `#958` as shipped and bounds Step3 to closure only.
4. Confirm the move-map only freezes ownership and does not propose another split.
5. Confirm the reviewer script re-runs the pinned approval/taxonomy/fulfillment/replay invariants.
