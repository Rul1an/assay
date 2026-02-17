# Wave7C Step3 review pack: judge/json_strict closure

Intent:
- Finalize Wave7C by closing facades behind existing internal modules.
- Keep behavior/perf/API stable while relocating tests away from facades.

Executed validation:
```bash
cargo fmt --check
cargo clippy -p assay-core -p assay-evidence --all-targets -- -D warnings
cargo check -p assay-core -p assay-evidence
BASE_REF=origin/main bash scripts/ci/review-wave7c-step3.sh
```

Anchor test paths after relocation:
```bash
cargo test -p assay-core --lib judge::judge_internal::tests::contract_two_of_three_majority -- --exact
cargo test -p assay-core --lib judge::judge_internal::tests::contract_sprt_early_stop -- --exact
cargo test -p assay-core --lib judge::judge_internal::tests::contract_abstain_mapping -- --exact
cargo test -p assay-core --lib judge::judge_internal::tests::contract_determinism_parallel_replay -- --exact

cargo test -p assay-evidence --lib json_strict::json_strict_internal::tests::test_rejects_top_level_duplicate -- --exact
cargo test -p assay-evidence --lib json_strict::json_strict_internal::tests::test_rejects_unicode_escape_duplicate -- --exact
cargo test -p assay-evidence --lib json_strict::json_strict_internal::tests::test_signature_duplicate_key_attack -- --exact
cargo test -p assay-evidence --lib json_strict::json_strict_internal::tests::test_dos_nesting_depth_limit -- --exact
cargo test -p assay-evidence --lib json_strict::json_strict_internal::tests::test_string_length_over_limit_rejected -- --exact
```

Facade closure proof snippets:
```rust
// /Users/roelschuurkes/assay/crates/assay-core/src/judge/mod.rs
pub async fn evaluate(...) -> anyhow::Result<()> {
    judge_internal::run::evaluate_impl(...).await
}
```

```rust
// /Users/roelschuurkes/assay/crates/assay-evidence/src/json_strict/mod.rs
pub fn from_str_strict<T: DeserializeOwned>(s: &str) -> Result<T, StrictJsonError> {
    json_strict_internal::run::from_str_strict_impl(s)
}

pub fn validate_json_strict(s: &str) -> Result<(), StrictJsonError> {
    json_strict_internal::run::validate_json_strict_impl(s)
}
```

Public surface snapshot (unchanged):
```bash
rg -n '^pub (fn|struct|enum|type|const)' crates/assay-core/src/judge/mod.rs crates/assay-evidence/src/json_strict/mod.rs
rg -n '^pub use ' crates/assay-evidence/src/json_strict/mod.rs
```

LOC snapshot:
- `/Users/roelschuurkes/assay/crates/assay-core/src/judge/mod.rs`: `408 -> 71` (-337)
- `/Users/roelschuurkes/assay/crates/assay-evidence/src/json_strict/mod.rs`: `493 -> 81` (-412)

Risk:
- Low: closure-only change; no public API/signature changes; boundary and allowlist gates remain strict.
