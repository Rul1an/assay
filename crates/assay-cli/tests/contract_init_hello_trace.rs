use assert_cmd::Command;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_init_hello_trace_contract() {
    let temp = tempdir().expect("temp dir");

    #[allow(deprecated)]
    let mut init = Command::cargo_bin("assay").expect("assay binary");
    init.current_dir(temp.path())
        .arg("init")
        .arg("--hello-trace")
        .assert()
        .success();

    let eval_path = temp.path().join("eval.yaml");
    let trace_path = temp.path().join("traces/hello.jsonl");
    assert!(eval_path.exists(), "eval.yaml must exist");
    assert!(trace_path.exists(), "traces/hello.jsonl must exist");

    let eval = fs::read_to_string(&eval_path).expect("read eval.yaml");
    assert!(
        eval.contains("suite: \"hello_smoke\""),
        "hello suite must be scaffolded"
    );
    assert!(
        eval.contains("id: \"hello_smoke_regex\""),
        "hello smoke test must be scaffolded"
    );

    #[allow(deprecated)]
    let mut validate = Command::cargo_bin("assay").expect("assay binary");
    validate
        .current_dir(temp.path())
        .arg("validate")
        .arg("--config")
        .arg("eval.yaml")
        .arg("--trace-file")
        .arg("traces/hello.jsonl")
        .assert()
        .success();
}
