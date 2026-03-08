# Mandate Types Step3 Checklist (Closure)

Scope lock:
- `docs/contributing/SPLIT-CHECKLIST-mandate-types-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-mandate-types-step3.md`
- `scripts/ci/review-mandate-types-step3.sh`
- no code movement in Step3
- no workflow changes

## Closure requirements

- re-run Step2 quality checks
- re-run Step2 facade/boundary/test invariants
- Step3 diff allowlist enforced
- stacked-base mode supported (`BASE_REF=origin/codex/wave18-mandate-types-step2-mechanical`)
- promote-precheck mode supported (`BASE_REF=origin/main`)

## Gate requirements

- `cargo fmt --check`
- `cargo clippy -p assay-evidence --all-targets -- -D warnings`
- `cargo test -p assay-evidence --lib mandate::types::tests::test_mandate_kind_serialization -- --exact`
- `cargo test -p assay-evidence --lib mandate::types::tests::test_mandate_builder -- --exact`
- `cargo test -p assay-evidence --lib mandate::types::tests::test_operation_class_serialization -- --exact`
- closure allowlist (`step2 -> step3`) docs+script only
- workflow-ban

## Definition of done

- reviewer script passes with:
  - `BASE_REF=origin/codex/wave18-mandate-types-step2-mechanical`
  - `BASE_REF=origin/main` (promote precheck)
