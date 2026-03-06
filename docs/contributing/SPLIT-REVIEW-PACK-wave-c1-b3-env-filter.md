# Wave C1 B3 Review Pack - env_filter.rs Mechanical Split

## Intent

Mechanically split `env_filter.rs` into focused modules while preserving strict/scrub/passthrough behavior and public surface.

## Scope

- `crates/assay-cli/src/env_filter.rs` (removed)
- `crates/assay-cli/src/env_filter/mod.rs`
- `crates/assay-cli/src/env_filter/engine.rs`
- `crates/assay-cli/src/env_filter/matcher.rs`
- `crates/assay-cli/src/env_filter/patterns.rs`
- `crates/assay-cli/src/env_filter/tests.rs`
- `docs/contributing/SPLIT-CHECKLIST-wave-c1-b3-env-filter.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave-c1-b3-env-filter.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave-c1-b3-env-filter.md`
- `scripts/ci/review-wave-c1-b3-env-filter.sh`

## Non-goals

- No filter behavior changes.
- No workflow changes.

## Validation Command

```bash
BASE_REF=<c1-b2-commit> bash scripts/ci/review-wave-c1-b3-env-filter.sh
```

Gate includes:

```bash
cargo fmt --check
cargo clippy -p assay-cli --all-targets -- -D warnings
cargo test -p assay-cli
```

## Reviewer 60s Scan

1. Confirm `mod.rs` is thin facade with re-exports.
2. Confirm pattern constants + matcher + filter engine are each single-source.
3. Confirm allowlist/workflow-ban/drift gates are strict.
4. Run reviewer script and confirm PASS.
