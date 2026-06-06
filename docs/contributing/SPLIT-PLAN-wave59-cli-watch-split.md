# Wave59 CLI Watch Split Plan

Goal:
- Mechanically split `crates/assay-cli/src/cli/commands/watch.rs` behind a stable command facade.
- Keep watch CLI behavior, output strings, debounce semantics, file snapshot logic, and run argument mapping unchanged.
- Defer `profile.rs` to a separate Wave60 PR.

Baseline:
- `crates/assay-cli/src/cli/commands/watch.rs`: 598 LOC on `origin/main` before Wave59.
- Existing inline tests cover watch path collection, debounce clamp behavior, snapshot hashing, diffing, coalescing, parse-error fallback, and `WatchArgs` -> `RunArgs` mapping.

Split shape:
- `watch.rs`: stable facade that re-exports `run`.
- `watch_next/mod.rs`: watch loop, debounce normalization, `run_once`, and `WatchArgs` -> `RunArgs` mapping.
- `watch_next/paths.rs`: watch target collection and refresh reporting.
- `watch_next/snapshot.rs`: file snapshots, small-file hashing, path diffing, and coalescing.
- `watch_next/tests.rs`: moved inline tests.

Non-goals:
- No watch-loop behavior changes.
- No CLI output or exit-code changes.
- No profile command changes.
- No `run` command changes.
- No Cargo, workflow, or dependency changes.

Review posture:
- Review as a move-only command split.
- Any watch behavior changes belong in a follow-up PR with CLI/output contracts.
