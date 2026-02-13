# Wave 2 Step 2 move map (function-first)

Scope:
- `crates/assay-core/src/engine/runner.rs`
- `crates/assay-core/src/runtime/mandate_store.rs`

All public entrypoints remain in facade files. Bodies moved mechanically to
`*_next` modules.

## Runner (`engine::runner`)

| Old symbol | New implementation |
| --- | --- |
| `Runner::run_suite` | `runner_next::execute::run_suite_impl` |
| `Runner::call_llm` | `runner_next::execute::call_llm_impl` |
| `Runner::check_baseline_regressions` | `runner_next::baseline::check_baseline_regressions_impl` |
| `Runner::embed_text` | `runner_next::cache::embed_text_impl` |
| `Runner::enrich_semantic` | `runner_next::scoring::enrich_semantic_impl` |
| `Runner::enrich_judge` | `runner_next::scoring::enrich_judge_impl` |
| `run_test_with_policy` path | `runner_next::execute::run_test_with_policy_impl` |
| `run_attempt_with_policy` path | `runner_next::execute::run_attempt_with_policy_impl` |
| `error_row_and_output` path | `runner_next::errors::error_row_and_output_impl` |
| attempt recording/classification path | `runner_next::retry::*` |

### Runner private/high-risk helpers

| Helper | New location | Called by |
| --- | --- | --- |
| `record_attempt` | `runner_next::retry::record_attempt_impl` | `runner_next::execute::run_test_with_policy_impl` |
| `should_stop_retries` | `runner_next::retry::should_stop_retries_impl` | `runner_next::execute::run_test_with_policy_impl` |
| `no_attempts_row` | `runner_next::retry::no_attempts_row_impl` | `runner_next::execute::run_test_with_policy_impl` |
| `apply_failure_classification` | `runner_next::retry::apply_failure_classification_impl` | `runner_next::execute::run_test_with_policy_impl` |
| `apply_quarantine_overlay` | `runner_next::execute::apply_quarantine_overlay_impl` | `runner_next::execute::run_test_with_policy_impl` |
| `empty_output_for_model` | `runner_next::execute::empty_output_for_model_impl` | `runner_next::execute::run_test_with_policy_impl` |
| `resolve_threshold_config` | `runner_next::baseline::resolve_threshold_config_impl` | `runner_next::baseline::check_baseline_regressions_impl` |

## Mandate store (`runtime::mandate_store`)

| Old symbol | New implementation |
| --- | --- |
| `MandateStore::open` | `mandate_store_next::schema::open_impl` |
| `MandateStore::memory` | `mandate_store_next::schema::memory_impl` |
| `MandateStore::from_connection` | `mandate_store_next::schema::from_connection_impl` |
| `MandateStore::upsert_mandate` | `mandate_store_next::upsert::upsert_mandate_impl` |
| `MandateStore::consume_mandate` | `mandate_store_next::txn::consume_mandate_in_txn_impl` |
| `MandateStore::consume_mandate_inner` | `mandate_store_next::consume::consume_mandate_inner_impl` |
| `MandateStore::get_use_count` | `mandate_store_next::stats::get_use_count_impl` |
| `MandateStore::count_uses` | `mandate_store_next::stats::count_uses_impl` |
| `MandateStore::nonce_exists` | `mandate_store_next::stats::nonce_exists_impl` |
| `MandateStore::upsert_revocation` | `mandate_store_next::revocation::upsert_revocation_impl` |
| `MandateStore::get_revoked_at` | `mandate_store_next::revocation::get_revoked_at_impl` |
| `MandateStore::is_revoked` | `mandate_store_next::revocation::is_revoked_impl` |
| `compute_use_id` | `mandate_store_next::stats::compute_use_id_impl` |

### Mandate store private/high-risk helpers

| Helper | New location | Called by |
| --- | --- | --- |
| `init_connection` | `mandate_store_next::schema::init_connection_impl` | `schema::open_impl`, `schema::memory_impl`, `schema::from_connection_impl` |
| `consume_mandate_inner` | `mandate_store_next::consume::consume_mandate_inner_impl` | `mandate_store_next::txn::consume_mandate_in_txn_impl` |
| transaction boundary (`BEGIN/COMMIT/ROLLBACK`) | `mandate_store_next::txn::consume_mandate_in_txn_impl` | `MandateStore::consume_mandate` |
| deterministic `use_id` hashing | `mandate_store_next::stats::compute_use_id_impl` | `consume::consume_mandate_inner_impl`, facade `compute_use_id` |
