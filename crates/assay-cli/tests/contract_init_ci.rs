use assert_cmd::Command;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_init_ci_contract() {
    let temp = tempdir().unwrap();
    let _current_dir = std::env::current_dir().unwrap();

    // Ensure we can run the binary
    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("assay").unwrap();

    // Run assay init --ci
    cmd.current_dir(temp.path())
        .arg("init")
        .arg("--ci")
        .assert()
        .success();

    // Verify file content
    let workflow_path = temp.path().join(".github/workflows/assay.yml");
    assert!(workflow_path.exists());

    let content = fs::read_to_string(workflow_path).unwrap();

    // Contract assertions
    assert!(
        content.contains("uses: Rul1an/assay/assay-action@v2"),
        "Must use blessed v2 action"
    );
    assert!(
        content.contains("security-events: write"),
        "Must request security-events: write permission"
    );
    assert!(
        content.contains("For strict supply-chain pinning"),
        "Must include pinning advice"
    );
    assert!(
        !content.contains("curl -fsSL"),
        "Must not use pipe-to-shell installation"
    );
}
