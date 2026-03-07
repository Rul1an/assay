# Agentic Step1 Checklist (Freeze)

Scope lock:
- docs + reviewer gate script only
- no workflow changes
- no edits under `crates/assay-core/src/agentic/**`

## Required outputs

- `docs/contributing/SPLIT-PLAN-wave12-agentic.md`
- `docs/contributing/SPLIT-CHECKLIST-agentic-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-agentic-step1.md`
- `scripts/ci/review-agentic-step1.sh`

## Freeze requirements

- public API freeze for `agentic` documented
- Step2 target module layout documented
- Step4 promote flow documented
- no tracked changes in `crates/assay-core/src/agentic/**`
- no untracked files in `crates/assay-core/src/agentic/**`

## Gate requirements

- `cargo fmt --check`
- `cargo clippy -p assay-core --all-targets -- -D warnings`
- `cargo test -p assay-core --lib agentic::tests::test_deduplication -- --exact`
- `cargo test -p assay-core --lib agentic::tests::test_detect_policy_shape -- --exact`
- `cargo test -p assay-core --lib agentic::tests::test_tool_poisoning_action_uses_assay_config_not_policy -- --exact`
- allowlist-only diff
- workflow-ban

## Definition of done

- reviewer script passes with `BASE_REF=origin/main`
- Step1 diff is limited to the four freeze files
