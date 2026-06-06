# Wave59 CLI Watch Split Checklist

Scope:
- [x] `watch.rs` remains the public command facade.
- [x] `run` remains available as `crate::cli::commands::watch::run`.
- [x] Watch path collection moved to `watch_next/paths.rs`.
- [x] Snapshot/diff/hash helpers moved to `watch_next/snapshot.rs`.
- [x] Existing unit tests moved to `watch_next/tests.rs`.
- [x] `profile.rs` remains untouched.

Behavior freeze:
- [x] No debounce bounds changed.
- [x] No polling interval changed.
- [x] No watch output strings changed.
- [x] No file hash size limit changed.
- [x] No `WatchArgs` -> `RunArgs` field mapping changed.
- [x] No config parse fallback behavior changed.

Validation:
- [x] `cargo fmt --check`
- [x] `cargo check -p assay-cli`
- [x] `cargo test -p assay-cli watch`
- [ ] `cargo clippy -p assay-cli --all-targets -- -D warnings`
- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave59-cli-watch-split.sh`

Review notes:
- `watch_next` is nested under the `watch` facade via `#[path = "watch_next/mod.rs"]`.
- `FileSnapshot` fields are `pub(super)` only so moved sibling tests can assert exact state without exposing new crate API.
