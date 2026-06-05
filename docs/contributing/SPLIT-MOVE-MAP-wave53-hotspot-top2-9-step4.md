# SPLIT MOVE MAP - Wave53 Step4 - Runner and eBPF Split

## Stack Base

Step4 is stacked on Step3:

- base: `codex/wave53-hotspot-top2-9-step3`
- head: `codex/wave53-hotspot-top2-9-step4`

Review Step4 against the Step3 branch, not directly against `main`, so earlier Wave53 movement does
not obscure the runner/eBPF split.

## Mechanical Movement

### Kernel Layer

Facade:

- `crates/assay-runner-core/src/kernel.rs`

Moved implementation:

- `crates/assay-runner-core/src/kernel/decode.rs`
- `crates/assay-runner-core/src/kernel/stats.rs`
- `crates/assay-runner-core/src/kernel/health.rs`
- `crates/assay-runner-core/src/kernel/notes.rs`
- `crates/assay-runner-core/src/kernel/tests.rs`

The facade preserves `KERNEL_EVENT_SCHEMA`, `KernelLayerEvent`, `KernelLayerCapture`,
`KernelLayerError`, and `KernelLayerBuilder`. Decode, stats, health downgrade, note formatting, and
existing tests move behind private child modules.

### eBPF Monitor

Facade:

- `crates/assay-ebpf/src/main.rs`

Moved implementation:

- `crates/assay-ebpf/src/open_events.rs`
- `crates/assay-ebpf/src/connect_events.rs`
- `crates/assay-ebpf/src/fork_events.rs`
- `crates/assay-ebpf/src/path_filter.rs`

`main.rs` keeps the tracepoint entrypoints and eBPF map declarations in place to preserve program
and map names. Helper modules own the moved open/connect/send/fork/path-filter implementation.

## Explicit Non-Movement

- No edits under `.github/workflows/**`.
- No edits to `crates/assay-ebpf/src/vmlinux.rs`.
- No edits to Wave53 Step5 policy target files.
- No eBPF tracepoint name, map name, payload, runner health-note, cgroup correlation, or event
  decoding behavior changes.

## LOC Snapshot

| Area | Before facade LOC | After facade LOC | New implementation modules |
| --- | ---: | ---: | --- |
| `kernel.rs` | 1404 | 267 | `decode.rs` 163, `stats.rs` 169, `health.rs` 52, `notes.rs` 110, `tests.rs` 687 |
| `assay-ebpf/src/main.rs` | 678 | 275 | `open_events.rs` 175, `connect_events.rs` 166, `fork_events.rs` 40, `path_filter.rs` 50 |
