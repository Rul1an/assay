# Tool Call Handler Step1 Review Pack (Freeze)

## Intent

Freeze Wave16 scope for `crates/assay-core/src/mcp/tool_call_handler.rs` before any mechanical moves.

## Scope

- `docs/contributing/SPLIT-PLAN-wave16-tool-call-handler.md`
- `docs/contributing/SPLIT-CHECKLIST-tool-call-handler-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-tool-call-handler-step1.md`
- `scripts/ci/review-tool-call-handler-step1.sh`

## Non-goals

- no changes under `crates/assay-core/src/mcp/**`
- no workflow changes
- no behavior or API changes

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-tool-call-handler-step1.sh
```

Gate includes:

```bash
cargo fmt --check
cargo clippy -p assay-core --all-targets -- -D warnings
cargo test -p assay-core tool_taxonomy_policy_match_handler_decision_event_records_classes -- --exact
cargo test -p assay-core test_event_contains_required_fields -- --exact
```

## Reviewer 60s scan

1. Confirm diff is only the 4 Step1 files.
2. Confirm workflow-ban and mcp subtree bans exist in the script.
3. Confirm targeted tests are pinned with `--exact`.
4. Run reviewer script and expect PASS.
