# Wave7B Step1 symbol snapshot (loader + store)

## `crates/assay-evidence/src/lint/packs/loader.rs`

Public constants/types:
- `PackSource`
- `LoadedPack`
- `PackError`

Public API surface:
- `LoadedPack::canonical_rule_id`
- `load_pack`
- `load_packs`
- `load_pack_from_file`

## `crates/assay-core/src/storage/store.rs`

Public constants/types:
- `Store`
- `StoreStats`

Public API surface:
- `Store::open`
- `Store::memory`
- `Store::init_schema`
- `Store::fetch_recent_results`
- `Store::fetch_results_for_last_n_runs`
- `Store::get_latest_run_id`
- `Store::fetch_results_for_run`
- `Store::get_last_passing_by_fingerprint`
- `Store::insert_run`
- `Store::create_run`
- `Store::finalize_run`
- `Store::insert_result_embedded`
- `Store::quarantine_get_reason`
- `Store::quarantine_add`
- `Store::quarantine_remove`
- `Store::cache_get`
- `Store::cache_put`
- `Store::get_embedding`
- `Store::put_embedding`
- `Store::stats_best_effort`
- `Store::get_episode_graph`
- `Store::insert_event`
- `Store::insert_batch`
- `Store::count_rows`
- `Store::get_latest_episode_graph_by_test_id`

Intent:
- Keep this file-local public surface stable during Wave7B Step1 and Step2.
