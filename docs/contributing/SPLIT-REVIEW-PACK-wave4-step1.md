# Review Pack: Wave4 Step1 (lockfile/cache behavior freeze)

Intent:
- Freeze behavior and drift surface for `lockfile.rs` and `cache.rs` before mechanical splits.
- No behavior/perf/code-path changes in this step.

Scope:
- `crates/assay-registry/src/lockfile.rs`
- `crates/assay-registry/src/cache.rs`
- `docs/contributing/SPLIT-INVENTORY-wave4-step1.md`
- `docs/contributing/SPLIT-SYMBOLS-wave4-step1.md`
- `docs/contributing/SPLIT-CHECKLIST-lockfile-step1.md`
- `docs/contributing/SPLIT-CHECKLIST-cache-step1.md`
- `scripts/ci/review-wave4-step1.sh`
- `docs/architecture/PLAN-split-refactor-2026q1.md`

Policy knobs:
- Base override: `BASE_REF` supported for stacked review.
- Drift gates are no-increase only.
- Diff allowlist is hard-fail.

Verification command:
```bash
BASE_REF=origin/codex/wave3-step1-behavior-freeze-v2 bash scripts/ci/review-wave4-step1.sh
```

Executed commands and outcomes:
- `cargo fmt --check` -> PASS
- `cargo clippy -p assay-registry --all-targets -- -D warnings` -> PASS
- `cargo check -p assay-registry` -> PASS
- `cargo test -p assay-registry test_lockfile_v2_roundtrip -- --nocapture` -> PASS
- `cargo test -p assay-registry test_lockfile_stable_ordering -- --nocapture` -> PASS
- `cargo test -p assay-registry test_lockfile_digest_mismatch_detection -- --nocapture` -> PASS
- `cargo test -p assay-registry test_lockfile_signature_fields -- --nocapture` -> PASS
- `cargo test -p assay-registry test_cache_roundtrip -- --nocapture` -> PASS
- `cargo test -p assay-registry test_cache_integrity_failure -- --nocapture` -> PASS
- `cargo test -p assay-registry test_signature_json_corrupt_handling -- --nocapture` -> PASS
- `cargo test -p assay-registry test_atomic_write_prevents_partial_cache -- --nocapture` -> PASS
- Drift gates + allowlist -> PASS

Drift counters (`before -> after`):
- `lockfile.rs`
  - `unwrap/expect`: `0 -> 0`
  - `unsafe`: `0 -> 0`
  - `println/eprintln`: `0 -> 0`
  - `dbg/trace/debug`: `3 -> 3`
  - `panic/todo/unimplemented`: `0 -> 0`
- `cache.rs`
  - `unwrap/expect`: `0 -> 0`
  - `unsafe`: `0 -> 0`
  - `println/eprintln`: `0 -> 0`
  - `dbg/trace/debug`: `6 -> 6`
  - `OpenOptions|tempfile|rename(|fs::|std::fs`: `11 -> 11`
  - `panic/todo/unimplemented`: `0 -> 0`

Explicitly out-of-scope in Step1:
- No mechanical split (`lockfile.rs`/`cache.rs` stay in place).
- No behavior changes.
- No performance tuning.
- No dependency or Cargo changes.
