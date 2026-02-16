# Wave7B Step2 move-map: loader/store

Boundary legend:
- `loader_internal/run.rs`: loader orchestration and entrypoint implementation.
- `loader_internal/resolve.rs`: builtin/local path resolution and suggestion logic.
- `loader_internal/parse.rs`: YAML parse + parse diagnostics.
- `loader_internal/digest.rs`: canonical digest computation.
- `loader_internal/compat.rs`: version compatibility helpers.
- `store_internal/schema.rs`: migration and schema utility helpers.
- `store_internal/results.rs`: status/result conversion helpers.
- `store_internal/episodes.rs`: episode graph read helper.

Moved functions -> target file:
- `load_pack_impl`, `load_packs_impl`, `load_pack_from_file_impl` -> `loader_internal/run.rs`
- `get_builtin_pack_with_name_impl`, `try_load_from_config_dir_impl`, `get_config_pack_dir_impl`, `is_valid_pack_name_impl`, `suggest_similar_pack_impl`, `levenshtein_distance_impl` -> `loader_internal/resolve.rs`
- `load_pack_from_string_impl`, `format_yaml_error_impl` -> `loader_internal/parse.rs`
- `compute_pack_digest_impl` -> `loader_internal/digest.rs`
- `check_version_compatibility_impl`, `version_satisfies_impl` -> `loader_internal/compat.rs`
- `migrate_v030_impl`, `get_columns_impl`, `add_column_if_missing_impl` -> `store_internal/schema.rs`
- `status_to_outcome_impl`, `parse_attempts_impl`, `message_and_details_from_attempts_impl`, `row_to_test_result_impl`, `insert_run_row_impl` -> `store_internal/results.rs`
- `load_episode_graph_for_episode_id_impl` -> `store_internal/episodes.rs`

Facade call chains (current):
- `load_pack` -> `loader_internal::run::load_pack_impl` -> `resolve/parse/*`
- `load_pack_from_file` -> `loader_internal::run::load_pack_from_file_impl` -> `parse::load_pack_from_string_impl`
- `migrate_v030` -> `store_internal::schema::migrate_v030_impl`
- `row_to_test_result` -> `store_internal::results::row_to_test_result_impl`
- `load_episode_graph_for_episode_id` -> `store_internal::episodes::load_episode_graph_for_episode_id_impl`

Forbidden knowledge by file:
- `loader.rs`: no direct YAML/JCS/SHA256 internals in code-only paths.
- `store.rs`: no migration SQL helper internals and no episode graph SELECT bodies for moved helper.
- `loader_internal/*`: responsibilities are file-specific and script-enforced.
- `store_internal/*`: responsibilities are file-specific and script-enforced.
