# Wave59 CLI Watch Review Pack

PR type:
- `refactor(cli): split watch command facade`

Reviewer focus:
- Confirm `watch.rs` only re-exports the command entrypoint.
- Confirm watch output strings and constants did not change.
- Confirm `run_once` still calls the same run command implementation.
- Confirm moved tests still cover path collection, debounce, snapshot, diff, coalesce, fallback, and RunArgs mapping.
- Confirm `profile.rs` is untouched.

Expected changed paths:
- `crates/assay-cli/src/cli/commands/watch.rs`
- `crates/assay-cli/src/cli/commands/watch_next/mod.rs`
- `crates/assay-cli/src/cli/commands/watch_next/paths.rs`
- `crates/assay-cli/src/cli/commands/watch_next/snapshot.rs`
- `crates/assay-cli/src/cli/commands/watch_next/tests.rs`
- `docs/contributing/SPLIT-PLAN-wave59-cli-watch-split.md`
- `docs/contributing/SPLIT-CHECKLIST-wave59-cli-watch-split.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave59-cli-watch-split.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave59-cli-watch-split.md`
- `scripts/ci/review-wave59-cli-watch-split.sh`

Verification commands:
- `cargo fmt --check`
- `cargo check -p assay-cli`
- `cargo test -p assay-cli watch`
- `cargo clippy -p assay-cli --all-targets -- -D warnings`
- `BASE_REF=origin/main bash scripts/ci/review-wave59-cli-watch-split.sh`

Merge posture:
- Merge only after local gate, GitHub checks, and review-comment sweep are clean.
