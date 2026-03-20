# SPLIT REVIEW PACK — Wave O1 Ringbuf Telemetry Step1

## Intent
Make ring-buffer pressure observable in the Linux runtime monitor path without changing monitor policy semantics or broadening the current stacked scope.

## Allowed files
- `crates/assay-common/src/lib.rs`
- `crates/assay-ebpf/src/main.rs`
- `crates/assay-ebpf/src/lsm.rs`
- `crates/assay-ebpf/src/socket_lsm.rs`
- `crates/assay-monitor/Cargo.toml`
- `crates/assay-monitor/src/lib.rs`
- `crates/assay-monitor/src/loader.rs`
- `crates/assay-cli/src/cli/commands/monitor_next/mod.rs`
- `docs/contributing/SPLIT-INVENTORY-wave-o1-ringbuf-telemetry-step1.md`
- `docs/contributing/SPLIT-CHECKLIST-wave-o1-ringbuf-telemetry-step1.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave-o1-ringbuf-telemetry-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave-o1-ringbuf-telemetry-step1.md`
- `scripts/ci/review-wave-o1-ringbuf-telemetry-step1.sh`

## What reviewers should verify
1. The diff stays inside the allowlist above.
2. Shared stat-slot constants are defined once in `assay-common`.
3. Every kernel ring-buffer producer now distinguishes reserve success vs failure.
4. Userspace can read the new counters without changing monitor attach/listen behavior.
5. The CLI prints a deterministic summary and a clear warning when drops occur.
6. No `assay-core` runner or OTel policy-eval work appears in this step.

## Proof snippets
- Kernel counters:
  - `MONITOR_STAT_TRACEPOINT_RINGBUF_DROPPED`
  - `MONITOR_STAT_LSM_RINGBUF_DROPPED`
  - `SOCKET_STAT_RINGBUF_DROPPED`
- Userspace export:
  - `MonitorStatsSnapshot`
  - `snapshot_stats()`
- CLI surfacing:
  - `Monitor summary:`
  - `Ring buffer pressure detected`

## Reviewer command
```bash
BASE_REF=origin/codex/codebase-analysis-followups bash scripts/ci/review-wave-o1-ringbuf-telemetry-step1.sh
```

## Validation note
The reviewer script runs the monitor and CLI checks directly. The eBPF target check is conditional and prints `SKIP` when `bpfel-unknown-none` is unavailable locally.
