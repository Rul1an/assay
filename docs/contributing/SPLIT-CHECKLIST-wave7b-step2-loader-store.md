# Wave7B Step2 checklist: loader/store mechanical split

Scope lock:
- Keep public signatures in `crates/assay-evidence/src/lint/packs/loader.rs` unchanged.
- Keep public signatures in `crates/assay-core/src/storage/store.rs` unchanged.
- Mechanical moves only: internal module extraction + facade delegation/wrappers.

Artifacts:
- `docs/contributing/SPLIT-CHECKLIST-wave7b-step2-loader-store.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave7b-step2-loader-store.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave7b-step2-loader-store.md`
- `scripts/ci/review-wave7b-step2.sh`

Runbook:
```bash
BASE_REF=origin/main bash scripts/ci/review-wave7b-step2.sh
```

Hard gates (script-enforced):
- `cargo fmt --check`
- `cargo clippy -p assay-evidence -p assay-core --all-targets -- -D warnings`
- `cargo check -p assay-evidence -p assay-core`
- Step1 loader/store anchor tests remain green.
- Loader facade delegates public entrypoints to `loader_internal::run::*`.
- Store facade delegates moved helpers to `store_internal::{schema,results,episodes}`.
- Single-source boundaries:
  - `loader_internal/resolve.rs`: source resolution + suggestions only.
  - `loader_internal/parse.rs`: YAML parse + parse error shaping only.
  - `loader_internal/digest.rs`: digest canonicalization only.
  - `loader_internal/compat.rs`: version compatibility only.
  - `loader_internal/run.rs`: orchestration only.
  - `store_internal/schema.rs`: migration/schema helper only.
  - `store_internal/results.rs`: result/status mapping helper only.
  - `store_internal/episodes.rs`: episode graph read helper only.
- Strict diff allowlist.

Definition of done:
- reviewer script passes on `BASE_REF=origin/main`
- no anchor regressions
- no scope leakage outside allowlist
