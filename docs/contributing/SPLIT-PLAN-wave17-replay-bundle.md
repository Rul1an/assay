# Wave17 Plan — `replay/bundle.rs` Split

## Goal

Split `crates/assay-core/src/replay/bundle.rs` into bounded modules with zero behavior change and stable public API/contracts.

## Step1 (freeze)

Branch: `codex/wave17-replay-bundle-step1-freeze` (base: `main`)

Deliverables:
- `docs/contributing/SPLIT-PLAN-wave17-replay-bundle.md`
- `docs/contributing/SPLIT-CHECKLIST-replay-bundle-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-replay-bundle-step1.md`
- `scripts/ci/review-replay-bundle-step1.sh`

Step1 constraints:
- docs+gate only
- no edits under `crates/assay-core/src/replay/**`
- no workflow edits

Step1 gate:
- allowlist-only diff (the 4 Step1 files)
- workflow-ban (`.github/workflows/*`)
- hard fail tracked changes in `crates/assay-core/src/replay/**`
- hard fail untracked files in `crates/assay-core/src/replay/**`
- `cargo fmt --check`
- `cargo clippy -p assay-core --all-targets -- -D warnings`
- targeted exact tests:
  - `cargo test -p assay-core --lib replay::bundle::tests::write_bundle_minimal_roundtrip -- --exact`
  - `cargo test -p assay-core --lib replay::bundle::tests::bundle_digest_equals_sha256_of_written_bytes -- --exact`
  - `cargo test -p assay-core --lib replay::verify::tests::verify_clean_bundle_passes -- --exact`

## Step2 (mechanical split preview)

Target layout (preview):
- `crates/assay-core/src/replay/bundle/mod.rs` (facade + public API)
- `crates/assay-core/src/replay/bundle/manifest.rs`
- `crates/assay-core/src/replay/bundle/verify.rs`
- `crates/assay-core/src/replay/bundle/io.rs`
- `crates/assay-core/src/replay/bundle/paths.rs`
- `crates/assay-core/src/replay/bundle/tests.rs` (or `tests/mod.rs`)

Step2 principles:
- 1:1 body moves
- stable public surface and manifest shape
- no hash/content-address semantic drift
- no behavior changes in verify/error categorization

## Step3 (closure)

Docs+gate-only closure slice that re-runs Step2 invariants and keeps allowlist strict.

## Promote

Stacked chain:
- Step1 -> `main`
- Step2 -> Step1
- Step3 -> Step2

Final promote PR to `main` from Step3 once chain is clean.
