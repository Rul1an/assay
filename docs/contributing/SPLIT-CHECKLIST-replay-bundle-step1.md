# Replay Bundle Step1 Checklist (Freeze)

Scope lock:
- `docs/contributing/SPLIT-PLAN-wave17-replay-bundle.md`
- `docs/contributing/SPLIT-CHECKLIST-replay-bundle-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-replay-bundle-step1.md`
- `scripts/ci/review-replay-bundle-step1.sh`
- no code edits under `crates/assay-core/src/replay/**`
- no workflow edits

## Gate expectations

- allowlist-only diff vs `BASE_REF` (default `origin/main`)
- workflow-ban (`.github/workflows/*`)
- hard fail tracked changes in `crates/assay-core/src/replay/**`
- hard fail untracked files in `crates/assay-core/src/replay/**`
- `cargo fmt --check`
- `cargo clippy -p assay-core --all-targets -- -D warnings`
- targeted exact tests:
  - `replay::bundle::tests::write_bundle_minimal_roundtrip`
  - `replay::bundle::tests::bundle_digest_equals_sha256_of_written_bytes`
  - `replay::verify::tests::verify_clean_bundle_passes`

## Definition of done

- `BASE_REF=origin/main bash scripts/ci/review-replay-bundle-step1.sh` passes
- Step1 diff contains only the 4 allowlisted files
