# Wave7B Step1 inventory (loader + store freeze)

Scope:
- `crates/assay-evidence/src/lint/packs/loader.rs`
- `crates/assay-core/src/storage/store.rs`

Scope lock:
- tests + docs + gates only
- no mechanical split/move in this step
- no behavior/perf changes
- `demo/` untouched

## Snapshot

- snapshot commit (this PR): `796f7c9d`
- LOC:
  - `loader.rs`: 793
  - `store.rs`: 774

## Public entrypoints (current)

`loader.rs`
- `PackSource`
- `LoadedPack`
- `LoadedPack::canonical_rule_id`
- `PackError`
- `load_pack`
- `load_packs`
- `load_pack_from_file`

`store.rs`
- `Store`
- `StoreStats`
- `Store::{open,memory,init_schema,fetch_recent_results,fetch_results_for_last_n_runs,get_latest_run_id,fetch_results_for_run,get_last_passing_by_fingerprint,insert_run,create_run,finalize_run,insert_result_embedded,quarantine_get_reason,quarantine_add,quarantine_remove,cache_get,cache_put,get_embedding,put_embedding,stats_best_effort,get_episode_graph,insert_event,insert_batch,count_rows,get_latest_episode_graph_by_test_id}`

## Baseline drift counters (code-only, test blocks excluded)

`loader.rs`
- `unwrap(` / `expect(`: 0
- `unsafe`: 0
- `println!/eprintln!/print!/dbg!/tracing::(debug|trace)!`: 0
- `panic!/todo!/unimplemented!`: 0
- IO footprint: `tokio::fs|std::fs|OpenOptions|rename(|create_dir_all|tempfile` => 3
- process/network: `Command::new|std::process|tokio::process|reqwest|hyper` => 0

`store.rs`
- `unwrap(` / `expect(`: 23
- `unsafe`: 0
- `println!/eprintln!/print!/dbg!/tracing::(debug|trace)!`: 0
- `panic!/todo!/unimplemented!`: 0
- IO footprint: `tokio::fs|std::fs|OpenOptions|rename(|create_dir_all|tempfile` => 0
- process/network: `Command::new|std::process|tokio::process|reqwest|hyper` => 0
