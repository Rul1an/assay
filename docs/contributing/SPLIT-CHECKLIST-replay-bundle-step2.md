# Replay Bundle Step2 Checklist (Mechanical)

Scope lock:
- `crates/assay-core/src/replay/bundle.rs` (delete)
- `crates/assay-core/src/replay/bundle/mod.rs`
- `crates/assay-core/src/replay/bundle/io.rs`
- `crates/assay-core/src/replay/bundle/manifest.rs`
- `crates/assay-core/src/replay/bundle/verify.rs`
- `crates/assay-core/src/replay/bundle/paths.rs`
- `crates/assay-core/src/replay/bundle/tests.rs`
- `docs/contributing/SPLIT-CHECKLIST-replay-bundle-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-replay-bundle-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-replay-bundle-step2.md`
- `scripts/ci/review-replay-bundle-step2.sh`

## Mechanical invariants

- `mod.rs` is facade-only (types, module wiring, re-exports).
- tar/gzip read/write logic is only in `io.rs`.
- digest logic is only in `verify.rs`.
- path validation helpers are only in `paths.rs`.
- file-manifest shaping logic is only in `manifest.rs`.
- tests moved from facade into `tests.rs` with same names.
- no workflow edits.

## Gate expectations

- allowlist-only diff vs `BASE_REF` (default `origin/main`)
- workflow-ban (`.github/workflows/*`)
- untracked-ban under `crates/assay-core/src/replay/bundle/**`
- `cargo fmt --check`
- `cargo clippy -p assay-core --all-targets -- -D warnings`
- targeted exact tests:
  - `replay::bundle::tests::write_bundle_minimal_roundtrip`
  - `replay::bundle::tests::bundle_digest_equals_sha256_of_written_bytes`
  - `replay::verify::tests::verify_clean_bundle_passes`

## Definition of done

- `BASE_REF=origin/main bash scripts/ci/review-replay-bundle-step2.sh` passes
- split remains behavior-identical (no API/path drift)
