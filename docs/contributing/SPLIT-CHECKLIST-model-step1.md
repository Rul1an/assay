# Model Step1 Checklist (Freeze)

Scope lock:
- docs + reviewer gate script only
- no workflow changes
- no edits under `crates/assay-core/src/model.rs` or `crates/assay-core/src/model/**`

## Required outputs

- `docs/contributing/SPLIT-PLAN-wave13-model.md`
- `docs/contributing/SPLIT-CHECKLIST-model-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-model-step1.md`
- `scripts/ci/review-model-step1.sh`

## Freeze requirements

- Step2 target layout documented
- Step4 promote flow documented
- helper-module no-IO guard documented
- no tracked changes in `crates/assay-core/src/model.rs` or `crates/assay-core/src/model/**`
- no untracked files in `crates/assay-core/src/model/**`

## Gate requirements

- `cargo fmt --check`
- `cargo clippy -p assay-core --all-targets -- -D warnings`
- `cargo test -p assay-core --lib model::tests::test_string_input_deserialize -- --exact`
- `cargo test -p assay-core --lib model::tests::test_legacy_list_expected -- --exact`
- `cargo test -p assay-core --lib model::tests::test_validate_ref_in_v1 -- --exact`
- allowlist-only diff
- workflow-ban

## Definition of done

- reviewer script passes with `BASE_REF=origin/main`
- Step1 diff is limited to the four freeze files
