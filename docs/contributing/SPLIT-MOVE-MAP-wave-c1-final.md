# SPLIT MOVE MAP - Wave C1 Final Layout

## CLI Args (`crates/assay-cli/src/cli/args`)

- Facade: `mod.rs` (`Cli`, `Command`, `ToolArgs` + module wiring/re-exports)
- Add new command args near domain modules:
  - replay flow args -> `replay.rs`
  - setup/discovery/mcp wrapper args -> `mcp.rs`
  - run/watch/calibrate/fix/sandbox args -> `runtime.rs`
  - import/init/migrate/demo args -> `import.rs`
  - coverage flags -> `coverage.rs`
  - evidence command shell -> `evidence.rs`
  - sim args -> `sim.rs`

## Replay Command (`crates/assay-cli/src/cli/commands/replay`)

- Facade: `mod.rs` (`pub use flow::run`)
- Command orchestration: `flow.rs`
- RunArg construction: `run_args.rs`
- Failure/exit writing: `failure.rs`
- Manifest/path resolution: `manifest.rs`
- FS/workspace/seed overrides: `fs_ops.rs`
- Provenance output annotation: `provenance.rs`
- Unit tests: `tests.rs`

## Env Filter (`crates/assay-cli/src/env_filter`)

- Facade: `mod.rs` (public re-exports)
- Filter engine and mode semantics: `engine.rs`
- Glob matcher: `matcher.rs`
- Pattern catalogs: `patterns.rs`
- Unit tests: `tests.rs`

## Reviewer Orientation (60s)

1. Find feature area in this map.
2. Confirm facade stayed thin.
3. Inspect only the owning module for behavior changes.
