# SPLIT INVENTORY — Wave O1 Ringbuf Telemetry Step1

## Stack context
- Base branch: `codex/codebase-analysis-followups`
- Step branch: `codex/codebase-analysis-observability`
- Intent: export ring-buffer emit/drop pressure from kernel monitor paths through userspace and surface it in `assay monitor`

## Scope lock
- In scope:
  - shared monitor/socket stat IDs in `assay-common`
  - kernel-side emit/drop counters in `assay-ebpf`
  - userspace stats snapshot + summary rendering in `assay-monitor` and `assay-cli`
  - monitor crate build hygiene needed to validate the touched code path
- Out of scope:
  - policy-evaluation OpenTelemetry spans
  - registry trust-root work
  - Python SDK, fuzzing, SBOM, docs positioning, workflows
  - new CLI flags or changed monitor attach semantics

## Touched implementation files
- `crates/assay-common/src/lib.rs`
- `crates/assay-ebpf/src/main.rs`
- `crates/assay-ebpf/src/lsm.rs`
- `crates/assay-ebpf/src/socket_lsm.rs`
- `crates/assay-monitor/Cargo.toml`
- `crates/assay-monitor/src/lib.rs`
- `crates/assay-monitor/src/loader.rs`
- `crates/assay-cli/src/cli/commands/monitor_next/mod.rs`

## Public surface inventory
- New public type: `assay_monitor::MonitorStatsSnapshot`
- New public method: `assay_monitor::Monitor::snapshot_stats()`
- Existing CLI surface preserved:
  - no new flags
  - no removed flags
  - monitor prints an end-of-run summary only after the event loop exits

## LOC baseline vs current

| File | Base LOC | Current LOC | Delta |
|---|---:|---:|---:|
| `crates/assay-common/src/lib.rs` | 197 | 211 | +14 |
| `crates/assay-ebpf/src/main.rs` | 273 | 289 | +16 |
| `crates/assay-ebpf/src/lsm.rs` | 257 | 269 | +12 |
| `crates/assay-ebpf/src/socket_lsm.rs` | 257 | 259 | +2 |
| `crates/assay-monitor/Cargo.toml` | 24 | 24 | +0 |
| `crates/assay-monitor/src/lib.rs` | 154 | 206 | +52 |
| `crates/assay-monitor/src/loader.rs` | 345 | 380 | +35 |
| `crates/assay-cli/src/cli/commands/monitor_next/mod.rs` | 369 | 401 | +32 |

## Validation target
- `cargo fmt --check`
- `cargo clippy -p assay-monitor -p assay-cli --all-targets -- -D warnings`
- `cargo check -p assay-monitor`
- `cargo test -p assay-monitor`
- `cargo check -p assay-cli`
- Optional: `cargo check -p assay-ebpf --features ebpf --target bpfel-unknown-none` when the target is installed
