# Wave47 Step2 Checklist

- [x] `checks.rs` reduced to a stable facade plus existing inline tests.
- [x] Check execution logic moved 1:1 into `checks_next/*` by responsibility.
- [x] `schema.rs` and `schema_next/*` left untouched.
- [x] `crates/assay-evidence/tests/**` left untouched.
- [x] No pack payload or `packs/open/**` changes.
- [x] Reviewer script enforces scope allowlist and contract tests.
- [x] Local validation run:
  - `git diff --check`
  - `cargo fmt --all --check`
  - `cargo clippy -q -p assay-evidence --all-targets -- -D warnings`
  - `cargo check -q -p assay-evidence`
  - `BASE_REF=origin/main bash scripts/ci/review-wave47-pack-checks-step2.sh`
