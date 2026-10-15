//! An attached exec probe must be able to emit.
//!
//! The exec producer reads its filename through `KEY_OFFSET_EXEC_FILENAME` and has no fallback,
//! so a probe that is reported attached while that key is unset records nothing and still
//! advertises process coverage. Standalone `assay monitor` attaches without calling
//! `configure_defaults`, which is the path this exercises.
#![cfg(target_os = "linux")]

use assay_common::{exec_record_offset_in_bounds, KEY_OFFSET_EXEC_FILENAME};
use assay_monitor::probes::ProbeOutcome;
use assay_monitor::Monitor;
use std::path::PathBuf;

fn ebpf_object() -> PathBuf {
    std::env::var_os("ASSAY_EBPF_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/assay-ebpf.o")
        })
}

#[test]
#[ignore = "requires root, BPF LSM with BTF, and target/assay-ebpf.o from cargo xtask build-ebpf"]
fn attach_without_configure_defaults_leaves_no_unconfigured_exec_probe() {
    let path = ebpf_object();
    let mut monitor =
        Monitor::load_file(&path).unwrap_or_else(|e| panic!("load {}: {e}", path.display()));

    // No configure_defaults: the standalone `assay monitor` sequence.
    monitor.attach().expect("attach");

    let outcome = monitor.probe_attachment().outcome("sched_process_exec");
    let offset = monitor
        .get_config_u32(KEY_OFFSET_EXEC_FILENAME)
        .expect("read CONFIG");
    assert!(
        outcome != Some(ProbeOutcome::Attached) || exec_record_offset_in_bounds(offset),
        "sched_process_exec reported {outcome:?} with exec filename offset {offset}: \
         an attached exec probe without a usable offset emits nothing"
    );
}
