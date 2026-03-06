# SPLIT MOVE MAP - Wave C1 B1 (cli/args/mod.rs)

## Section Move Map

- `ValidateArgs`, `Quarantine*`, `Trace*` -> `crates/assay-cli/src/cli/args/replay.rs`
- `Init*`, `ImportArgs`, `MigrateArgs`, `DemoArgs` -> `crates/assay-cli/src/cli/args/import.rs`
- `CoverageArgs` -> `crates/assay-cli/src/cli/args/coverage.rs`
- `Mcp*`, `ConfigPathArgs`, `DiscoverArgs`, `SetupArgs` -> `crates/assay-cli/src/cli/args/mcp.rs`
- `CalibrateArgs`, `DoctorArgs`, `WatchArgs`, `FixArgs`, `SandboxArgs`, `MaxRisk` -> `crates/assay-cli/src/cli/args/runtime.rs`
- `Sim*` args -> `crates/assay-cli/src/cli/args/sim.rs`
- `EvidenceArgs` -> `crates/assay-cli/src/cli/args/evidence.rs`
- Top-level CLI surface (`Cli`, `Command`, `ToolArgs`) remains in `crates/assay-cli/src/cli/args/mod.rs`

## Symbol Map (old -> new)

- `ValidateArgs` -> `args/replay.rs`
- `ImportArgs` -> `args/import.rs`
- `CoverageArgs` -> `args/coverage.rs`
- `McpArgs`, `McpWrapArgs`, `DiscoverArgs`, `SetupArgs` -> `args/mcp.rs`
- `CalibrateArgs`, `DoctorArgs`, `WatchArgs`, `FixArgs`, `SandboxArgs` -> `args/runtime.rs`
- `SimArgs`, `SimRunArgs`, `SimSoakArgs` -> `args/sim.rs`
- `EvidenceArgs` -> `args/evidence.rs`

## Facade Contract

`crates/assay-cli/src/cli/args/mod.rs` keeps the command entrypoint and re-exports module symbols, preserving existing `super::args::*` call sites.
