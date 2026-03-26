# Wave46 Step2 Checklist

- [x] `schema.rs` reduced to a stable facade plus existing inline tests.
- [x] Schema logic moved 1:1 into `schema_next/*` by responsibility.
- [x] `checks.rs` left untouched.
- [x] `crates/assay-evidence/tests/**` left untouched.
- [x] No pack payload or `packs/open/**` changes.
- [x] Reviewer script enforces scope allowlist and contract tests.
- [x] Local validation run:
  - `git diff --check`
  - `cargo check -q -p assay-evidence`
  - `cargo fmt --all --check`
  - `cargo clippy -q -p assay-evidence --all-targets -- -D warnings`
  - `BASE_REF=origin/main bash scripts/ci/review-wave46-pack-schema-step2.sh`
