use assert_cmd::Command;
use std::fs;
use std::path::Path;

fn workspace_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

#[test]
fn docs_reference_includes_generate_and_explain_contract_flags() {
    let root = workspace_root();
    let index = fs::read_to_string(root.join("docs/reference/cli/index.md")).expect("read index");
    let explain =
        fs::read_to_string(root.join("docs/reference/cli/explain.md")).expect("read explain doc");
    let generate =
        fs::read_to_string(root.join("docs/reference/cli/generate.md")).expect("read generate doc");

    assert!(
        index.contains("[`assay generate`](generate.md)"),
        "CLI index must link to assay generate"
    );
    assert!(
        index.contains("[`assay explain`](explain.md)"),
        "CLI index must link to assay explain"
    );

    assert!(
        explain.contains("--compliance-pack"),
        "explain docs must include compliance-pack option"
    );
    assert!(
        generate.contains("--diff"),
        "generate docs must include diff option"
    );
}

#[test]
fn cli_help_exposes_generate_diff_and_explain_compliance_pack() {
    #[allow(deprecated)]
    let mut generate_help = Command::cargo_bin("assay").expect("assay binary");
    let generate_output = generate_help
        .arg("generate")
        .arg("--help")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let generate_stdout = String::from_utf8(generate_output).expect("utf8 generate help");
    assert!(
        generate_stdout.contains("--diff"),
        "generate help must expose --diff"
    );

    #[allow(deprecated)]
    let mut explain_help = Command::cargo_bin("assay").expect("assay binary");
    let explain_output = explain_help
        .arg("explain")
        .arg("--help")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let explain_stdout = String::from_utf8(explain_output).expect("utf8 explain help");
    assert!(
        explain_stdout.contains("--compliance-pack"),
        "explain help must expose --compliance-pack"
    );
}
