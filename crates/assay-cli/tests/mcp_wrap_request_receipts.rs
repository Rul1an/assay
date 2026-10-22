//! Runs bounded synthetic stdio requests through the real CLI and proxy.
#[test]
fn request_receipt_contract() {
    let directory = tempfile::tempdir().unwrap();
    let script = directory.path().join("test_receipts.py");
    std::fs::write(
        &script,
        include_str!("fixtures/request_receipts/test_receipts.py"),
    )
    .unwrap();
    let output = std::process::Command::new(if cfg!(windows) { "python" } else { "python3" })
        .arg(&script)
        .env("ASSAY_BIN", env!("CARGO_BIN_EXE_assay"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
