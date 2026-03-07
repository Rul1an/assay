# Model Step3 Checklist (Closure)

Scope lock:
- Step3 docs + Step3 gate script only
- no additional code movement
- no workflow changes

## Required outputs

- `docs/contributing/SPLIT-CHECKLIST-model-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-model-step3.md`
- `scripts/ci/review-model-step3.sh`

## Closure requirements

- re-run Step2 quality checks
- re-run facade/boundary/relocation invariants
- Step3 diff allowlist enforced
- promote-precheck mode supported (`BASE_REF=origin/main`)

## Gate requirements

- `cargo fmt --check`
- `cargo clippy -p assay-core --all-targets -- -D warnings`
- `cargo test -p assay-core --lib model::tests::test_string_input_deserialize -- --exact`
- `cargo test -p assay-core --lib model::tests::test_legacy_list_expected -- --exact`
- `cargo test -p assay-core --lib model::tests::test_validate_ref_in_v1 -- --exact`
- closure allowlist (`step2 -> step3`)
- promote allowlist (`main -> step3`)
- workflow-ban

## Definition of done

- reviewer script passes with:
  - `BASE_REF=origin/codex/wave13-model-step2-mechanical`
  - `BASE_REF=origin/main` (promote precheck)
