# Mandate Types Step2 Checklist (Mechanical)

Scope lock:
- `crates/assay-evidence/src/mandate/types.rs` (deleted)
- `crates/assay-evidence/src/mandate/types/mod.rs`
- `crates/assay-evidence/src/mandate/types/core.rs`
- `crates/assay-evidence/src/mandate/types/serde.rs`
- `crates/assay-evidence/src/mandate/types/schema.rs`
- `crates/assay-evidence/src/mandate/types/tests.rs`
- `docs/contributing/SPLIT-CHECKLIST-mandate-types-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-mandate-types-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-mandate-types-step2.md`
- `scripts/ci/review-mandate-types-step2.sh`
- no workflow edits

## Gate expectations

- allowlist-only diff vs `BASE_REF` (default `origin/main`)
- workflow-ban (`.github/workflows/*`)
- hard fail untracked files in `crates/assay-evidence/src/mandate/types/**`
- `cargo fmt --check`
- `cargo clippy -p assay-evidence --all-targets -- -D warnings`
- targeted exact tests:
  - `mandate::types::tests::test_mandate_kind_serialization`
  - `mandate::types::tests::test_mandate_builder`
  - `mandate::types::tests::test_operation_class_serialization`

## Mechanical invariants

- facade `types/mod.rs` only does wiring + re-exports
- `types/mod.rs` has no inline logic (`fn`/`impl`) and no inline `mod tests { ... }`
- no serde/schema helper logic in facade
- `types/core.rs`, `types/serde.rs`, `types/schema.rs` contain no fs/env/network IO markers
- public type paths remain via `mandate::types::*`
- test names are preserved in `types/tests.rs`

## Definition of done

- `BASE_REF=origin/main bash scripts/ci/review-mandate-types-step2.sh` passes
- Step2 diff contains only allowlisted files
