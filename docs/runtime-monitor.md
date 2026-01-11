## Runtime Monitor (assay-monitor) — Linux eBPF (Experimental / P1)

`assay-monitor` is the user-space component that streams runtime events from eBPF probes
(e.g. file opens and network connects) and can trigger alerts / kill switch actions.

### Key Properties

- **Cross-platform build**
  - On **Linux**, eBPF is enabled via `aya`.
  - On **macOS/Windows**, the crate compiles but all methods return `MonitorError::NotSupported`.
  - This avoids sprinkling `#[cfg(target_os = "linux")]` guards across the workspace.

- **Stable event streaming**
  - Uses a **threaded RingBuf reader** for maximum compatibility across Aya versions.
  - Events are bridged into `tokio::mpsc` and exposed as a `Stream`.

- **Hardened ABI**
  - `assay-common` contains `MonitorEvent` with **compile-time layout assertions**.
  - `assay-monitor` parses events via **MaybeUninit + memcpy**, avoiding UB.

### Usage (Linux)

```rust
use assay_monitor::Monitor;
use tokio_stream::StreamExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1) Load the eBPF bytecode
    let mut monitor = Monitor::load_file("/path/to/assay_ebpf.o")?;

    // 2) Configure which PIDs are monitored (optional)
    monitor.set_monitored_pids(&[1234, 5678])?;

    // 3) Attach probes (sys_enter_openat / sys_enter_connect)
    monitor.attach()?;

    // 4) Consume events
    let mut stream = monitor.listen()?;
    while let Some(ev) = stream.next().await {
        match ev {
            Ok(e) => println!("Event: pid={} type={}", e.pid, e.event_type),
            Err(err) => eprintln!("Monitor error: {err}"),
        }
    }

    Ok(())
}
```

### macOS / Windows behavior

On non-Linux hosts, Monitor exists but returns MonitorError::NotSupported for all runtime methods.
This allows builds/tests on dev laptops while runtime execution remains Linux-only.

### Notes
- Running requires elevated privileges (root / CAP_BPF / CAP_PERFMON).
- Building the eBPF object is Linux-first. Non-Linux hosts should use Docker/CI to produce assay_ebpf.o.

### Advanced Troubleshooting (Linux)

If `assay monitor` fails on Linux:

**A. Missing Artifact**
- **Error**: "eBPF object not found"
- **Fix**: Run `cargo xtask build-ebpf --release`. Check `target/assay-ebpf.o`.

**B. Attach Failed**
- **Check Symbols**: `llvm-objdump -t target/assay-ebpf.o | egrep 'assay_monitor_(openat|connect)'` (Must see both).
- **Check Tracepoints**:
  - `ls /sys/kernel/debug/tracing/events/syscalls/sys_enter_openat`
  - If missing, try mounting: `sudo mount -t tracefs nodev /sys/kernel/tracing`

**C. No Events**
- **Check PID**: Ensure `--pid` is a living process (TGID).
- **Privileges**: Use `sudo` or set caps:
  `sudo setcap cap_bpf,cap_perfmon,cap_sys_admin+ep ./target/release/assay`

### CI Automation

The `.github/workflows/ci.yml` includes an `ebpf-smoke` job that:
- Runs on `ubuntu-latest` (native Linux).
- Builds `assay-cli` and `assay-ebpf`.
- Verifies symbol presence with `llvm-objdump`.
- Runs a **smoke test** using a Python helper to trigger verified events (`openat`) under `sudo`.
