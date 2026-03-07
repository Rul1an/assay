# Agentic Step3 Checklist (Closure)

Scope lock:
- closure docs + reviewer script only
- no workflow changes
- no code movement

## Goal

Close Wave12 with strict closure gates while keeping Step2 split intact.

## Required outputs

- `docs/contributing/SPLIT-CHECKLIST-agentic-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-agentic-step3.md`
- `scripts/ci/review-agentic-step3.sh`

## Required checks

- `cargo fmt --check`
- `cargo clippy -p assay-core --all-targets -- -D warnings`
- `cargo test -p assay-core --lib agentic::tests::test_deduplication -- --exact`
- `cargo test -p assay-core --lib agentic::tests::test_detect_policy_shape -- --exact`
- `cargo test -p assay-core --lib agentic::tests::test_tool_poisoning_action_uses_assay_config_not_policy -- --exact`
- `BASE_REF=origin/codex/wave12-agentic-step2-mechanical bash scripts/ci/review-agentic-step3.sh`

## Closure invariants

- Step2 facade invariants remain true
- Step2 public surface invariants remain true
- Step2 builder visibility invariant remains true
- original 5 test names remain present
- Step3 diff limited to Step3 docs/script files only
- workflow-ban enforced

## Definition of done

- reviewer script passes
- Step3 diff is closure-only
