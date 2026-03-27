# T-R2 Step2 Checklist

- [x] `tests.rs` converted into a directory-backed unit-test tree rooted at `tests/mod.rs`.
- [x] Shared helpers moved into `tests/fixtures.rs`.
- [x] Scenario families moved 1:1 into `emission`, `delegation`, `approval`, `scope`, `redaction`, `classification`, and `lifecycle`.
- [x] `tests/mod.rs` reduced to module wiring only.
- [x] `crates/assay-core/tests/**` left untouched.
- [x] No production edits outside the `tool_call_handler` test tree.
- [x] Reviewer script enforces scope allowlist and module-qualified exact selectors.
- [x] Local validation run:
  - `git diff --check`
  - `cargo fmt --all --check`
  - `cargo clippy -q -p assay-core --all-targets -- -D warnings`
  - `cargo test -q -p assay-core --lib`
  - `BASE_REF=origin/main bash scripts/ci/review-tr2-tool-call-handler-tests-step2.sh`
