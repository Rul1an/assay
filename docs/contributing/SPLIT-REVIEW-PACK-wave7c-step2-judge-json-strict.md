# Wave7C Step2 review pack: judge/json_strict mechanical split

Intent:
- Mechanically split large helper/orchestration blocks from:
  - `/Users/roelschuurkes/assay/crates/assay-core/src/judge/mod.rs`
  - `/Users/roelschuurkes/assay/crates/assay-evidence/src/json_strict/mod.rs`
- Keep public behavior/signatures stable behind existing facades.

Executed validation:
```bash
cargo fmt --check
cargo clippy -p assay-core -p assay-evidence --all-targets -- -D warnings
cargo check -p assay-core -p assay-evidence
BASE_REF=origin/main bash scripts/ci/review-wave7c-step2.sh
```

Targeted anchor checks (also run inside reviewer script):
```bash
cargo test -p assay-core --lib judge::tests::contract_two_of_three_majority -- --exact
cargo test -p assay-core --lib judge::tests::contract_sprt_early_stop -- --exact
cargo test -p assay-core --lib judge::tests::contract_abstain_mapping -- --exact
cargo test -p assay-core --lib judge::tests::contract_determinism_parallel_replay -- --exact
cargo test -p assay-evidence --lib json_strict::tests::test_rejects_top_level_duplicate -- --exact
cargo test -p assay-evidence --lib json_strict::tests::test_rejects_unicode_escape_duplicate -- --exact
cargo test -p assay-evidence --lib json_strict::tests::test_signature_duplicate_key_attack -- --exact
cargo test -p assay-evidence --lib json_strict::tests::test_dos_nesting_depth_limit -- --exact
cargo test -p assay-evidence --lib json_strict::tests::test_string_length_over_limit_rejected -- --exact
```

Facade proof snippets:
```rust
// crates/assay-core/src/judge/mod.rs
pub async fn evaluate(...) -> anyhow::Result<()> {
    judge_internal::run::evaluate_impl(...).await
}
```

```rust
// crates/assay-evidence/src/json_strict/mod.rs
pub fn from_str_strict<T: DeserializeOwned>(s: &str) -> Result<T, StrictJsonError> {
    json_strict_internal::run::from_str_strict_impl(s)
}

pub fn validate_json_strict(s: &str) -> Result<(), StrictJsonError> {
    json_strict_internal::run::validate_json_strict_impl(s)
}
```

Single-source proof snippets (`rg`):
```bash
rg -n 'fn build_prompt_impl|const SYSTEM_PROMPT' crates/assay-core/src/judge/judge_internal -g'*.rs'
# judge_internal/prompt.rs only

rg -n 'async fn call_judge_impl|serde_json::from_str|LlmClient' crates/assay-core/src/judge/judge_internal -g'*.rs'
# judge_internal/client.rs only

rg -n 'fn inject_result_impl|fn generate_cache_key_impl' crates/assay-core/src/judge/judge_internal -g'*.rs'
# judge_internal/cache.rs only

rg -n 'impl JsonValidator|fn validate_value|fn validate_object|fn validate_array' crates/assay-evidence/src/json_strict/json_strict_internal -g'*.rs'
# json_strict_internal/validate.rs only

rg -n 'fn parse_json_string_impl|surrogate' crates/assay-evidence/src/json_strict/json_strict_internal -g'*.rs'
# json_strict_internal/decode.rs only
```

LOC snapshot:
- `/Users/roelschuurkes/assay/crates/assay-core/src/judge/mod.rs`: `712 -> 408` (-304)
- `/Users/roelschuurkes/assay/crates/assay-evidence/src/json_strict/mod.rs`: `759 -> 493` (-266)

Risk:
- Medium-low: mechanical extraction only; public signatures stable; Step1 anchors + boundary gates enforce parity.
