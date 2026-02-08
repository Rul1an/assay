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

    let ci_eval_path = temp.path().join("ci-eval.yaml");
    assert!(ci_eval_path.exists(), "ci-eval.yaml must be generated");
    let ci_eval = fs::read_to_string(ci_eval_path).unwrap();
    assert!(
        ci_eval.contains("configVersion: 1"),
        "CI eval scaffold must write canonical configVersion field"
    );
    assert!(
        ci_eval.contains("semantic_similarity_to: \"Hello Semantic\""),
        "CI eval scaffold must write canonical semantic_similarity_to field"
    );
    assert!(
        ci_eval.contains("min_score: 0.99"),
        "CI eval scaffold must write canonical min_score field"
    );
    assert!(
        !ci_eval
            .lines()
            .any(|line| line.trim_start().starts_with("version: 1")),
        "Legacy version alias should not be emitted in generated scaffold"
    );
    assert!(
        !ci_eval.contains("\n      text:"),
        "Legacy semantic field alias should not be emitted in generated scaffold"
    );
    assert!(
        !ci_eval.contains("\n      threshold:"),
        "Legacy threshold field alias should not be emitted in generated scaffold"
    );
}

#[test]
fn test_init_preset_and_pack_alias_contract() {
    let temp = tempdir().unwrap();

    #[allow(deprecated)]
    let mut preset_cmd = Command::cargo_bin("assay").unwrap();
    preset_cmd
        .current_dir(temp.path())
        .arg("init")
        .arg("--preset")
        .arg("hardened")
        .assert()
        .success();

    let policy_path = temp.path().join("policy.yaml");
    assert!(
        policy_path.exists(),
        "init --preset must create policy scaffold"
    );

    let temp_alias = tempdir().unwrap();
    #[allow(deprecated)]
    let mut pack_alias_cmd = Command::cargo_bin("assay").unwrap();
    pack_alias_cmd
        .current_dir(temp_alias.path())
        .arg("init")
        .arg("--pack")
        .arg("hardened")
        .assert()
        .success();

    let alias_policy_path = temp_alias.path().join("policy.yaml");
    assert!(
        alias_policy_path.exists(),
        "init --pack alias must remain backward-compatible"
    );
}
