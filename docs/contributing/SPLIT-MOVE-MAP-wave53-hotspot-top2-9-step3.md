# SPLIT MOVE MAP - Wave53 Step3 - CLI Command Split

## Stack Base

Step3 is stacked on Step2:

- base: `codex/wave53-hotspot-top2-9-step2`
- head: `codex/wave53-hotspot-top2-9-step3`

Review Step3 against the Step2 branch, not directly against `main`, so Step1 and Step2 movement do
not obscure the CLI command split.

## Mechanical Movement

### Runner-Spike Command

Facade:

- `crates/assay-cli/src/cli/commands/runner_spike.rs`

Moved implementation:

- `crates/assay-cli/src/cli/commands/runner_spike/args.rs`
- `crates/assay-cli/src/cli/commands/runner_spike/implementation.rs`
- `crates/assay-cli/src/cli/commands/runner_spike/spec.rs`
- `crates/assay-cli/src/cli/commands/runner_spike/phases.rs`
- `crates/assay-cli/src/cli/commands/runner_spike/cgroup.rs`
- `crates/assay-cli/src/cli/commands/runner_spike/logs.rs`
- `crates/assay-cli/src/cli/commands/runner_spike/exit_status.rs`

The facade preserves `runner_spike::RunnerSpikeArgs`, `RunnerSpikeCommand`, and
`RunnerSpikeRunArgs`, then delegates `run` to `implementation::run`.

### Doctor Command

Facade:

- `crates/assay-cli/src/cli/commands/doctor.rs`

Moved implementation:

- `crates/assay-cli/src/cli/commands/doctor/implementation.rs`
- `crates/assay-cli/src/cli/commands/doctor/fixes.rs`
- `crates/assay-cli/src/cli/commands/doctor/patching.rs`
- `crates/assay-cli/src/cli/commands/doctor/parse_error.rs`

The facade preserves the `doctor::run` entrypoint and delegates to `implementation::run`.

## Explicit Non-Movement

- No edits under `.github/workflows/**`.
- No edits to `crates/assay-ebpf/src/vmlinux.rs`.
- No edits to Wave53 Step4 or Step5 target files.
- No CLI output, exit-code, dependency, policy, eBPF, or behavior cleanup changes.

## LOC Snapshot

| Area | Before facade LOC | After facade LOC | New implementation modules |
| --- | ---: | ---: | --- |
| `runner_spike.rs` | 686 | 16 | `args.rs` 57, `implementation.rs` 41, `spec.rs` 34, `phases.rs` 64, `cgroup.rs` 429, `logs.rs` 57, `exit_status.rs` 34 |
| `doctor.rs` | 629 | 12 | `implementation.rs` 139, `fixes.rs` 213, `patching.rs` 98, `parse_error.rs` 204 |
