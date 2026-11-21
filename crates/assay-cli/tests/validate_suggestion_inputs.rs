#![cfg(target_os = "linux")]

use nix::errno::Errno;
use nix::sys::inotify::{AddWatchFlags, InitFlags, Inotify};
use std::{fs, process::Command};

#[test]
fn a_config_parse_error_does_not_read_an_unrelated_policy() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("assay.yaml"), "version: [\n").unwrap();
    let path = dir.path().join("policy.yaml");
    fs::write(&path, "tools: {allow: [read]}\n").unwrap();
    let watch = Inotify::init(InitFlags::IN_NONBLOCK | InitFlags::IN_CLOEXEC).unwrap();
    watch.add_watch(&path, AddWatchFlags::IN_ACCESS).unwrap();
    fs::read(&path).unwrap();
    assert!(watch
        .read_events()
        .unwrap()
        .iter()
        .any(|event| event.mask.contains(AddWatchFlags::IN_ACCESS)));

    let result = Command::new(env!("CARGO_BIN_EXE_assay"))
        .current_dir(dir.path())
        .args(["validate", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["diagnostics"][0]["code"], "E_CFG_PARSE");
    assert_eq!(report["suggested_actions"][0]["id"], "regen_config");
    assert!(
        matches!(watch.read_events(), Err(Errno::EAGAIN)),
        "rendering the config diagnostic read policy.yaml"
    );
}
