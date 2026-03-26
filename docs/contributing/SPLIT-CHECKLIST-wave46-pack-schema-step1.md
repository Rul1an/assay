# Wave46 Pack Schema Step1 Checklist (Freeze)

## Scope lock

- [ ] Only these files changed:
  - `docs/contributing/SPLIT-PLAN-wave46-pack-schema.md`
  - `docs/contributing/SPLIT-CHECKLIST-wave46-pack-schema-step1.md`
  - `docs/contributing/SPLIT-MOVE-MAP-wave46-pack-schema-step1.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave46-pack-schema-step1.md`
  - `scripts/ci/review-wave46-pack-schema-step1.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No edits under `crates/assay-evidence/src/lint/packs/**`
- [ ] No edits under `crates/assay-evidence/tests/**`
- [ ] No edits under `packs/open/**`

## Step1 freeze contract

- [ ] `schema.rs` is explicitly identified as the first `R4` target
- [ ] `checks.rs` is explicitly deferred until after the schema split
- [ ] `json_path_exists` / `value_equals` rules are frozen
- [ ] conditional validation and unsupported-shape boundaries are frozen
- [ ] built-in/open pack loadability and parity are frozen
- [ ] parsing / validation error categories and reason strings are treated as contract-sensitive

## Validation

- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave46-pack-schema-step1.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-evidence --all-targets -- -D warnings` passes
- [ ] targeted schema / conditional / loader parity tests pass
