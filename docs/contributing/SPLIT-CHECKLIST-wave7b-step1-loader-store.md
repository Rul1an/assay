# Wave7B Step1 checklist: loader + store freeze

Scope lock:
- Step1 is tests + docs + reviewer gates only.
- No mechanical split in this step.
- No behavior/perf changes in this step.

Artifacts:
- `docs/contributing/SPLIT-INVENTORY-wave7b-step1-loader-store.md`
- `docs/contributing/SPLIT-SYMBOLS-wave7b-step1-loader-store.md`
- `docs/contributing/SPLIT-CHECKLIST-wave7b-step1-loader-store.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave7b-step1-loader-store.md`
- `scripts/ci/review-wave7b-step1.sh`

Runbook:
```bash
BASE_REF=origin/main bash scripts/ci/review-wave7b-step1.sh
```

Hard gates (script-enforced):
- BASE_REF resolve guard + effective `BASE_REF`/`HEAD` SHA print.
- `cargo fmt --check`
- `cargo clippy -p assay-evidence --all-targets -- -D warnings`
- `cargo clippy -p assay-core --all-targets -- -D warnings`
- `cargo check -p assay-evidence -p assay-core`
- Loader/store contract-anchor tests.
- No-production-change gates for both hotspot files (code-only compare).
- File-local public-surface freeze gates for both hotspot files.
- No-increase drift counters for both files:
  - `unwrap/expect`
  - `unsafe`
  - print/debug/log macros
  - `panic/todo/unimplemented`
  - IO footprint (`tokio::fs|std::fs|OpenOptions|rename(|create_dir_all|tempfile`)
  - process/network (`Command::new|std::process|tokio::process|reqwest|hyper`)
- Strict diff allowlist for Step1 paths.

Definition of done:
- reviewer script passes on `BASE_REF=origin/main`
- no non-allowlisted file changes
- production paths in `loader.rs` and `store.rs` unchanged in Step1
