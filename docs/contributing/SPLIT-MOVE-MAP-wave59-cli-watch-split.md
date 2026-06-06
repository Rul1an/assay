# Wave59 CLI Watch Move Map

| Before | After | Notes |
| --- | --- | --- |
| `watch.rs::run` | `watch_next/mod.rs::run` | Re-exported by `watch.rs`. |
| Debounce constants | `watch_next/mod.rs` | Values unchanged. |
| `normalize_debounce_ms` | `watch_next/mod.rs` | Moved 1:1. |
| `run_once` | `watch_next/mod.rs` | Calls `crate::cli::commands::run::run` after nesting. |
| `run_args_from_watch` | `watch_next/mod.rs` | Field mapping unchanged. |
| `collect_watch_paths` | `watch_next/paths.rs` | Moved 1:1. |
| `refresh_watch_targets` | `watch_next/paths.rs` | Moved 1:1. |
| `FileSnapshot` | `watch_next/snapshot.rs` | Fields `pub(super)` for moved tests only. |
| `snapshot_paths` / content hash | `watch_next/snapshot.rs` | Moved 1:1. |
| `diff_paths` / `coalesce_changed_paths` | `watch_next/snapshot.rs` | Moved 1:1. |
| Inline `#[cfg(test)] mod tests` | `watch_next/tests.rs` | Moved 1:1 with imports adjusted. |

LOC delta:
- `crates/assay-cli/src/cli/commands/watch.rs`: 598 -> 4.
- New `watch_next/mod.rs`: 155.
- New `watch_next/paths.rs`: 65.
- New `watch_next/snapshot.rs`: 81.
- New `watch_next/tests.rs`: 311.

Deferred:
- `profile.rs` split remains Wave60.
- No CLI-output golden changes in this PR.
