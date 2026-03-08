# Replay Bundle Step1 Review Pack (Freeze)

## Intent

Freeze Wave17 scope for `crates/assay-core/src/replay/bundle.rs` before any mechanical moves.

## Scope

- `docs/contributing/SPLIT-PLAN-wave17-replay-bundle.md`
- `docs/contributing/SPLIT-CHECKLIST-replay-bundle-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-replay-bundle-step1.md`
- `scripts/ci/review-replay-bundle-step1.sh`

## Non-goals

- no changes under `crates/assay-core/src/replay/**`
- no workflow changes
- no behavior or API changes

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-replay-bundle-step1.sh
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

1. Confirm diff is only the 4 Step1 files.
2. Confirm workflow-ban and replay subtree bans exist in the script.
3. Confirm targeted tests are pinned with `--exact`.
4. Run reviewer script and expect PASS.
