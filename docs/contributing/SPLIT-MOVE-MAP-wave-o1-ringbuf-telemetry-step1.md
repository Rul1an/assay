# SPLIT MOVE MAP — Wave O1 Ringbuf Telemetry Step1

## Intent
This step is additive telemetry only. There is no facade split and no behavior move from one module tree to another.

## Data flow map
1. `crates/assay-common/src/lib.rs`
   - defines shared stat slot IDs for tracepoint, LSM, and socket monitor paths
2. `crates/assay-ebpf/src/main.rs`
   - increments tracepoint emit/drop counters when `EVENTS.reserve()` succeeds or fails
3. `crates/assay-ebpf/src/lsm.rs`
   - increments LSM emit/drop counters when `LSM_EVENTS.reserve()` succeeds or fails
4. `crates/assay-ebpf/src/socket_lsm.rs`
   - increments socket checks / allowed / blocked / emitted / dropped counters
5. `crates/assay-monitor/src/loader.rs`
   - reads `STATS` and `SOCKET_STATS` maps into a userspace snapshot
6. `crates/assay-monitor/src/lib.rs`
   - exposes `MonitorStatsSnapshot` and `Monitor::snapshot_stats()`
7. `crates/assay-cli/src/cli/commands/monitor_next/mod.rs`
   - renders the final monitor summary and explicit pressure warning

## Reviewer focus
- Counter IDs remain shared and consistent across kernel/userspace
- No event payload ABI changes are introduced here
- Summary output is post-loop only and does not alter live event handling
- `assay-monitor` build hygiene change is limited to enabling the `std` feature needed by the touched crate
