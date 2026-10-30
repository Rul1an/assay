use assert_cmd::Command;

#[test]
fn explain_html_escapes_trace_text_and_preserves_json_and_exit_codes() {
    let dir = tempfile::tempdir().unwrap();
    let trace = dir.path().join("trace.json");
    let policy = dir.path().join("policy.yaml");
    let tool = "</code><script>marker()</script>";
    std::fs::write(
        &trace,
        serde_json::to_vec(&serde_json::json!([{"tool":tool}])).unwrap(),
    )
    .unwrap();
    for (policy_text, exit) in [
        ("version: '1.1'\nname: html\n".to_owned(), 0),
        (
            format!("version: '1.1'\nname: html\ntools:\n  deny: ['{tool}']\n"),
            1,
        ),
    ] {
        std::fs::write(&policy, policy_text).unwrap();
        for format in ["html", "json"] {
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
            if format == "html" {
                let html = String::from_utf8(output).unwrap();
                assert!(html.contains("&lt;/code&gt;&lt;script&gt;marker()&lt;/script&gt;"));
                assert!(!html.contains("<script>"));
            } else {
                let report: serde_json::Value = serde_json::from_slice(&output).unwrap();
                assert_eq!(report["steps"][0]["tool"], tool);
            }
        }
    }
}
