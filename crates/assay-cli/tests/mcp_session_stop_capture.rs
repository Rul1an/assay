//! Workspace-only Linux capture/receiver contract, executed by the required cargo test lane.
#![cfg(target_os = "linux")]
use std::{path::Path, time::Duration};

#[test]
fn capture_and_cold_receiver_contract() {
    let kit =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/agent-session-evidence-probe");
    for required in [
        "test_session_stop.py",
        "test_session_stop_capture.py",
        "session_stop.py",
        "wrap_entry.py",
    ] {
        assert!(
            kit.join(required).is_file(),
            "missing receiver contract: {required}"
        );
    }
    assert_cmd::Command::new("python3")
        .timeout(Duration::from_secs(40))
        .env("ASSAY_BIN", env!("CARGO_BIN_EXE_assay"))
        .args(["-m", "unittest", "discover", "-s"])
        .arg(kit)
        .args(["-p", "test_session_stop*.py", "-v"])
        .assert()
        .success();
}
