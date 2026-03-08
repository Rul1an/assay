# Replay Bundle Step2 Review Pack (Mechanical Split)

## Intent

Perform Wave17 mechanical split of `crates/assay-core/src/replay/bundle.rs` into focused modules while preserving behavior and public API.

## Scope

- `crates/assay-core/src/replay/bundle.rs` (deleted)
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

## Non-goals

- no workflow changes
- no replay manifest schema redesign
- no digest/verification contract changes

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-replay-bundle-step2.sh
```

Gate includes:

```bash
cargo fmt --check
cargo clippy -p assay-core --all-targets -- -D warnings
cargo test -p assay-core --lib replay::bundle::tests::write_bundle_minimal_roundtrip -- --exact
cargo test -p assay-core --lib replay::bundle::tests::bundle_digest_equals_sha256_of_written_bytes -- --exact
cargo test -p assay-core --lib replay::verify::tests::verify_clean_bundle_passes -- --exact
```

## Reviewer 60s scan

1. Confirm diff is limited to Step2 allowlist.
2. Confirm `mod.rs` is thin facade only.
3. Confirm tar/gzip logic exists only in `io.rs`.
4. Confirm digest logic exists only in `verify.rs`.
5. Confirm targeted contract tests stay green.
