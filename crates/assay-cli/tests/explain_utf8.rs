use assert_cmd::Command;

#[test]
fn explain_utf8_summary_preserves_policy_exit_codes_and_json_args() {
    let dir = tempfile::tempdir().unwrap();
    let trace = dir.path().join("trace.json");
    let policy = dir.path().join("policy.yaml");
    std::fs::write(
        &trace,
        r#"[{"tool":"Search","args":{"query":"aaaaaaaaaaaaaaaaaaaé"}}]"#,
    )
    .unwrap();

    for (policy_text, exit) in [
        ("version: '1.1'\nname: utf8\n", 0),
        ("version: '1.1'\nname: utf8\ntools:\n  deny: [Search]\n", 1),
    ] {
        std::fs::write(&policy, policy_text).unwrap();
        for format in ["terminal", "json"] {
            let output = Command::new(assert_cmd::cargo::cargo_bin!("assay"))
                .args(["explain", "--trace"])
                .arg(&trace)
                .arg("--policy")
                .arg(&policy)
                .args(["--format", format])
                .timeout(std::time::Duration::from_secs(15))
                .assert()
                .code(exit)
                .get_output()
                .stdout
                .clone();
            if format == "terminal" {
                assert!(String::from_utf8(output)
                    .unwrap()
                    .contains("Search(query: \"aaaaaaaaaaaaaaaaaaa...\")"));
            } else {
                let report: serde_json::Value = serde_json::from_slice(&output).unwrap();
                assert_eq!(report["steps"][0]["args"]["query"], "aaaaaaaaaaaaaaaaaaaé");
            }
        }
    }
}
