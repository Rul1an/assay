//! An attached exec probe must be able to emit.
//!
//! The exec producer reads its filename through `KEY_OFFSET_EXEC_FILENAME` and has no fallback,
//! so a probe that is reported attached while that key is unset records nothing and still
//! advertises process coverage. Standalone `assay monitor` attaches without calling
//! `configure_defaults`, which is the path this exercises.
#![cfg(target_os = "linux")]

use assay_common::{exec_record_offset_in_bounds, KEY_OFFSET_EXEC_FILENAME};
use assay_monitor::Monitor;
use std::path::PathBuf;

fn ebpf_object() -> PathBuf {
    std::env::var_os("ASSAY_EBPF_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/assay-ebpf.o")
        })
}

// Loading creates aya's async perf-event array, which registers with the Tokio reactor, so the
// test runs inside a runtime as the CLI does.
#[tokio::test(flavor = "current_thread")]
#[ignore = "requires root, BPF LSM with BTF, and target/assay-ebpf.o from cargo xtask build-ebpf"]
async fn attach_without_configure_defaults_leaves_no_unconfigured_exec_probe() {
    let path = ebpf_object();
    let mut monitor =
        Monitor::load_file(&path).unwrap_or_else(|e| panic!("load {}: {e}", path.display()));

    // No configure_defaults: the standalone `assay monitor` sequence.
    monitor.attach().expect("attach");

    // Precondition, asserted on its own: the exec probe is in the attached list. `outcome()`
    // only reports mode-aware rows, so it cannot answer this for an Always probe.
    let attachment = monitor.probe_attachment();
    assert!(
        attachment.attached_probes().contains(&"sched_process_exec"),
        "sched_process_exec is not attached: {:?}",
        attachment.attached_probes()
    );

    let offset = monitor
        .get_config_u32(KEY_OFFSET_EXEC_FILENAME)
        .expect("read CONFIG");
    assert!(
        exec_record_offset_in_bounds(offset),
        "sched_process_exec is attached with exec filename offset {offset}: \
         an attached exec probe without a usable offset emits nothing"
    );
}
