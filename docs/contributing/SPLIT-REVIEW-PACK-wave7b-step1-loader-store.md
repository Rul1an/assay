# Review Pack: Wave7B Step1 (loader + store behavior freeze)

Intent:
- Freeze `loader.rs` + `store.rs` behavior before mechanical Wave7B split.
- Keep production code unchanged in Step1; only tests/docs/gates changes are allowed.

Scope:
- `crates/assay-evidence/src/lint/packs/loader.rs` (tests only if changed)
- `crates/assay-core/src/storage/store.rs` (tests only if changed)
- `docs/contributing/SPLIT-INVENTORY-wave7b-step1-loader-store.md`
- `docs/contributing/SPLIT-SYMBOLS-wave7b-step1-loader-store.md`
- `docs/contributing/SPLIT-CHECKLIST-wave7b-step1-loader-store.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave7b-step1-loader-store.md`
- `scripts/ci/review-wave7b-step1.sh`
- `docs/architecture/PLAN-split-refactor-2026q1.md`

## 1) Freeze anchors

Loader anchors:
- `test_local_pack_resolution`
- `test_builtin_wins_over_local`
- `test_local_invalid_yaml_fails`
- `test_resolution_order_mock`
- `test_path_wins_over_builtin`
- `test_symlink_escape_rejected`

Store anchors:
- `test_storage_smoke_lifecycle`
- `e1_runs_write_contract_insert_and_create`
- `e1_latest_run_selection_is_id_based_not_timestamp_string`
- `e1_stats_read_compat_keeps_legacy_started_at`

## 2) Public surface snapshot

Command:
```bash
rg -n '^\s*pub\s+(const|struct|enum|type|trait|fn)\b' crates/assay-evidence/src/lint/packs/loader.rs
rg -n '^\s*pub\s+(const|struct|enum|type|trait|fn)\b' crates/assay-core/src/storage/store.rs
```

Surface scope clarification:
- Step1 freezes file-local `pub` declarations in these hotspot files (not full crate-level re-export graph).

## 3) Step1 hard-fail gate definitions

`scripts/ci/review-wave7b-step1.sh` enforces:
- no-production-change (code-only compare vs `BASE_REF`, excluding `#[cfg(test)]` blocks)
- file-local public-surface freeze (pub symbol diff)
- no-increase drift counters (`unwrap/expect`, `unsafe`, print/debug/log, panic/todo/unimplemented, IO footprint, process/network)
- strict diff allowlist

Validation:
```bash
BASE_REF=origin/main bash scripts/ci/review-wave7b-step1.sh
```
