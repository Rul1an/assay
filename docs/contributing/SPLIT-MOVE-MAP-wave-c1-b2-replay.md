# SPLIT MOVE MAP - Wave C1 B2 (cli/commands/replay.rs)

## Section Move Map

- `run(...)` orchestration -> `crates/assay-cli/src/cli/commands/replay/flow.rs`
- `replay_run_args(...)` -> `crates/assay-cli/src/cli/commands/replay/run_args.rs`
- `write_missing_dependency(...)` + `write_replay_failure(...)` -> `crates/assay-cli/src/cli/commands/replay/failure.rs`
- `offline_dependency_message(...)`, config/trace resolution, source-run-id extraction -> `crates/assay-cli/src/cli/commands/replay/manifest.rs`
- seed override, atomic write, entry materialization, hash + workspace lifecycle -> `crates/assay-cli/src/cli/commands/replay/fs_ops.rs`
- replay provenance annotation helpers -> `crates/assay-cli/src/cli/commands/replay/provenance.rs`
- existing replay unit tests -> `crates/assay-cli/src/cli/commands/replay/tests.rs`
- thin facade + module wiring -> `crates/assay-cli/src/cli/commands/replay/mod.rs`

## Symbol Map (old -> new)

- `run` -> `replay/flow.rs`
- `replay_run_args` -> `replay/run_args.rs`
- `write_replay_failure` -> `replay/failure.rs`
- `write_missing_dependency` -> `replay/failure.rs`
- `offline_dependency_message` -> `replay/manifest.rs`
- `resolve_config_path` / `resolve_trace_path` -> `replay/manifest.rs`
- `apply_seed_override` -> `replay/fs_ops.rs`
- `ReplayWorkspace` -> `replay/fs_ops.rs`
- `annotate_replay_outputs` / `annotate_run_json_provenance` -> `replay/provenance.rs`

## Facade Contract

`crates/assay-cli/src/cli/commands/replay/mod.rs` re-exports `run` from `flow.rs`, preserving existing call sites and command dispatch behavior.
