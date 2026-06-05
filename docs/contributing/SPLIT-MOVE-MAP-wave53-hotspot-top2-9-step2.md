# SPLIT MOVE MAP - Wave53 Step2 - High-Readiness Hotspot Split

## Stack Base

Step2 is stacked on Step1:

- base: `codex/wave53-hotspot-top2-9-step1`
- head: `codex/wave53-hotspot-top2-9-step2`

Review Step2 against the Step1 branch, not directly against `main`, so the Step1 freeze artifacts do
not obscure the mechanical split.

## Mechanical Movement

### Summary Report

Facade:

- `crates/assay-core/src/report/summary.rs`

Moved implementation:

- `crates/assay-core/src/report/summary/types.rs`
- `crates/assay-core/src/report/summary/metrics.rs`
- `crates/assay-core/src/report/summary/writer.rs`

The facade keeps `report::summary` stable and re-exports the moved public types plus
`judge_metrics_from_results` and `write_summary`.

### Bundle Command

Facade:

- `crates/assay-cli/src/cli/commands/bundle.rs`

Moved implementation:

- `crates/assay-cli/src/cli/commands/bundle/implementation.rs`
- `crates/assay-cli/src/cli/commands/bundle/verify.rs`
- `crates/assay-cli/src/cli/commands/bundle/paths.rs`
- `crates/assay-cli/src/cli/commands/bundle/coverage.rs`

The facade keeps the `bundle` command entrypoint stable and delegates to `implementation::run`.

### Lockfile

Facade:

- `crates/assay-registry/src/lockfile.rs`

Moved implementation:

- `crates/assay-registry/src/lockfile_next/types.rs`

The facade continues to expose `Lockfile`, `LockedPack`, `LockSource`, `LockSignature`,
`VerifyLockResult`, and `LockMismatch` through `pub use lockfile_next::types`.

## Explicit Non-Movement

- No edits under `.github/workflows/**`.
- No edits to `crates/assay-ebpf/src/vmlinux.rs`.
- No edits to Wave53 Step3, Step4, or Step5 target files.
- No behavior cleanup, dependency changes, output changes, or serialization changes.

## LOC Snapshot

| Area | Before facade LOC | After facade LOC | New implementation modules |
| --- | ---: | ---: | --- |
| `summary.rs` | 629 | 12 | `types.rs` 544, `metrics.rs` 79, `writer.rs` 10 |
| `bundle.rs` | 632 | 14 | `implementation.rs` 246, `verify.rs` 29, `paths.rs` 220, `coverage.rs` 156 |
| `lockfile.rs` | 649 | 544 | `lockfile_next/types.rs` 111 |
