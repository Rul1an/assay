# SAFETY MAP - Wave54 eBPF Unsafe Documentation

## Intent

Document the invariant classes for hand-written unsafe code in the eBPF kernel path and runner cgroup assignment path.

## Non-Goals

- Do not edit `crates/assay-ebpf/src/vmlinux.rs`; it is generated bindgen output.
- Do not migrate the workspace to edition 2024.
- Do not change map capacities, tracepoint entrypoints, event schemas, filtering behavior, cgroup assignment behavior, or LSM decisions.
- Do not make `assay-ebpf` full `clippy -D warnings` clean in this PR; unrelated Clippy cleanup belongs in a separate PR.

## eBPF

| File | Baseline unsafe lines | Safety classes |
| --- | ---: | --- |
| `crates/assay-ebpf/src/main.rs` | 10 | map access, BPF helper calls, event header raw pointer writes, panic handler, generated `vmlinux` exemption |
| `crates/assay-ebpf/src/open_events.rs` | 21 | tracepoint reads, map access, ring buffer writes, pending-open raw pointer access |
| `crates/assay-ebpf/src/connect_events.rs` | 9 | tracepoint reads, msghdr probe reads, ring buffer writes |
| `crates/assay-ebpf/src/fork_events.rs` | 5 | tracepoint reads, ring buffer writes |
| `crates/assay-ebpf/src/lsm.rs` | 21 | LSM hook args, kernel pointer reads, map access, ring buffer writes |
| `crates/assay-ebpf/src/socket_lsm.rs` | 13 | BPF helpers, socket hook args, map access, byte reinterpretation, ring buffer writes |

## Runner Cgroup

| File | Baseline unsafe lines | Safety classes |
| --- | ---: | --- |
| `crates/assay-cli/src/cli/commands/runner_spike/cgroup.rs` | 5 | `pre_exec`, `getpid`, `open`, `write`, `close` for cgroup assignment |

## Safety Invariant Classes

- eBPF map access: unsafe because Aya exposes map operations through unsafe APIs; key lifetimes are local and missing keys are handled.
- Tracepoint reads: unsafe because kernel tracepoint context reads depend on configured offsets; read failures return an error or safe default.
- Ring buffer writes: unsafe because reserved ring-buffer memory is written through raw pointers; entries are initialized before submit.
- Kernel pointer reads: unsafe because LSM hook pointers come from kernel context; null and probe-read failures are handled before use.
- BPF helpers: unsafe because helper calls cross the verifier boundary; scalar results are not dereferenced.
- Byte reinterpretation: unsafe only when source and target layouts are same-size byte representations with no invalid `u8` bit patterns.
- Pre-exec cgroup assignment: unsafe because `pre_exec` runs after fork and before exec; the closure only calls async-signal-safe libc operations.

## Generated Non-Target

`crates/assay-ebpf/src/vmlinux.rs` is generated bindgen output and is excluded from this wave. It is exempted through the module declaration in `crates/assay-ebpf/src/main.rs`, not by editing generated code.
