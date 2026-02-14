# Wave4 Step1 inventory (lockfile/cache behavior freeze)

Scope:
- `crates/assay-registry/src/lockfile.rs`
- `crates/assay-registry/src/cache.rs`

Snapshot:
- HEAD: `91dffdbb`
- Base for Step1 drift checks: `origin/main`

LOC:
- `crates/assay-registry/src/lockfile.rs`: `863`
- `crates/assay-registry/src/cache.rs`: `844`

Drift counters (best-effort code-only, tests/comments filtered):
- `lockfile.rs`
  - `unwrap/expect`: `0`
  - `unsafe`: `0`
  - `println/eprintln`: `0`
  - `dbg/trace/debug`: `3`
  - `panic/todo/unimplemented`: `0`
- `cache.rs`
  - `unwrap/expect`: `0`
  - `unsafe`: `0`
  - `println/eprintln`: `0`
  - `dbg/trace/debug`: `6`
  - `OpenOptions|tempfile|rename(|fs::|std::fs`: `11`
  - `panic/todo/unimplemented`: `0`

Contract-freeze test anchors:
- Lockfile:
  - `test_lockfile_v2_roundtrip`
  - `test_lockfile_stable_ordering`
  - `test_lockfile_digest_mismatch_detection`
  - `test_lockfile_signature_fields`
- Cache:
  - `test_cache_roundtrip`
  - `test_cache_integrity_failure`
  - `test_signature_json_corrupt_handling`
  - `test_atomic_write_prevents_partial_cache`

Notes:
- Step1 is docs/gates/verification only; no mechanical split in this step.
- Counter filter is conservative; false positives acceptable, false negatives possible until tests are externalized.
