//! Runs bounded synthetic stdio requests through the real CLI and proxy.
use std::path::PathBuf;

fn workspace_doc(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn receipts_guide_records_only_jsonrpc_call_id() {
    let guide = workspace_doc("docs/guides/request-decision-receipts.md");
    assert!(
        !guide.contains("keys are used only when those members are present and well-typed"),
        "guide must not describe requestState/inputRequests keys as link identifiers"
    );
    assert!(
        guide.contains("not extracted") && guide.contains("call_id"),
        "guide must say the recorded link is JSON-RPC call_id, not requestState keys"
    );
    let adr = workspace_doc("docs/architecture/ADR-042-evidence-first-positioning.md");
    assert!(
        !adr.contains("inputRequests` keys when those members are present and well-typed"),
        "ADR-042 note must not list requestState/inputRequests as recorded-beside identifiers"
    );
}

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
