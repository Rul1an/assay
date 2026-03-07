# Agentic Step1 Review Pack (Freeze)

## Intent

Freeze Wave12 scope for `agentic` split before any code movement.

## Scope

- `docs/contributing/SPLIT-PLAN-wave12-agentic.md`
- `docs/contributing/SPLIT-CHECKLIST-agentic-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-agentic-step1.md`
- `scripts/ci/review-agentic-step1.sh`

## Non-goals

- no edits in `crates/assay-core/src/agentic/**`
- no behavior changes
- no workflow changes

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-agentic-step1.sh
```

Gate includes:

```bash
cargo fmt --check
cargo clippy -p assay-core --all-targets -- -D warnings
cargo test -p assay-core --lib agentic::tests::test_deduplication -- --exact
cargo test -p assay-core --lib agentic::tests::test_detect_policy_shape -- --exact
cargo test -p assay-core --lib agentic::tests::test_tool_poisoning_action_uses_assay_config_not_policy -- --exact
```

## Reviewer 60s scan

1. Confirm only Step1 docs/script changed.
2. Confirm no `agentic/**` tracked/untracked changes.
3. Confirm Step2/Step4 process is explicitly documented.
4. Run reviewer script and expect PASS.
