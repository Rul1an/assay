# Mandate Types Step1 Checklist (Freeze)

Scope lock:
- `docs/contributing/SPLIT-PLAN-wave18-mandate-types.md`
- `docs/contributing/SPLIT-CHECKLIST-mandate-types-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-mandate-types-step1.md`
- `scripts/ci/review-mandate-types-step1.sh`
- no code edits under `crates/assay-evidence/src/mandate/**`
- no workflow edits

## Gate expectations

- allowlist-only diff vs `BASE_REF` (default `origin/main`)
- workflow-ban (`.github/workflows/*`)
- hard fail tracked changes in `crates/assay-evidence/src/mandate/**`
- hard fail untracked files in `crates/assay-evidence/src/mandate/**`
- `cargo fmt --check`
- `cargo clippy -p assay-evidence --all-targets -- -D warnings`
- targeted exact tests:
  - `mandate::types::tests::test_mandate_kind_serialization`
  - `mandate::types::tests::test_mandate_builder`
  - `mandate::types::tests::test_operation_class_serialization`

## Definition of done

- `BASE_REF=origin/main bash scripts/ci/review-mandate-types-step1.sh` passes
- Step1 diff contains only the 4 allowlisted files
