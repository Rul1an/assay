use super::*;
#[test]
fn schema_cli_validates_jsonl_inputs() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("results.jsonl");
    fs::write(
        &input,
        concat!(
            r#"{"gradingResult":{"componentResults":[{"pass":true,"score":1,"reason":"Assertion passed","assertion":{"type":"equals","value":"Hello world"}}]}}"#,
            "\n"
        ),
    )
    .unwrap();

    let output = assay_schema_command()
        .arg("validate")
        .arg("--schema")
        .arg("promptfoo-cli-jsonl-component-result.v1")
        .arg("--input")
        .arg(&input)
        .arg("--jsonl")
        .arg("--format")
        .arg("json")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(report["schema"], "assay.evidence.schema.validation.v1");
    assert_eq!(report["valid"], true);
    assert_eq!(report["documents"], 1);
}

#[test]
fn schema_cli_returns_exit_one_for_schema_mismatch() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("receipt.json");
    fs::write(
        &input,
        json!({
            "schema": "assay.receipt.promptfoo.assertion-component.v1",
            "source_system": "promptfoo"
        })
        .to_string(),
    )
    .unwrap();

    let output = assay_schema_command()
        .arg("validate")
        .arg("--schema")
        .arg("promptfoo.assertion-component.v1")
        .arg("--input")
        .arg(&input)
        .arg("--format")
        .arg("json")
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let report: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(report["valid"], false);
    assert!(!report["errors"].as_array().unwrap().is_empty());
}

#[test]
fn schema_cli_returns_exit_two_for_invalid_json_input() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("receipt.json");
    fs::write(
        &input,
        r#"{"schema":"assay.receipt.promptfoo.assertion-component.v1""#,
    )
    .unwrap();

    let output = assay_schema_command()
        .arg("validate")
        .arg("--schema")
        .arg("promptfoo.assertion-component.v1")
        .arg("--input")
        .arg(&input)
        .arg("--format")
        .arg("json")
        .assert()
        .code(2)
        .get_output()
        .stdout
        .clone();
    let report: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(report["valid"], false);
    assert_eq!(report["errors"][0]["kind"], "parse");
}

#[test]
fn schema_cli_returns_exit_two_for_empty_jsonl_input() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("results.jsonl");
    fs::write(&input, "\n\n").unwrap();

    let output = assay_schema_command()
        .arg("validate")
        .arg("--schema")
        .arg("promptfoo-cli-jsonl-component-result.v1")
        .arg("--input")
        .arg(&input)
        .arg("--jsonl")
        .arg("--format")
        .arg("json")
        .assert()
        .code(2)
        .get_output()
        .stdout
        .clone();
    let report: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(report["valid"], false);
    assert_eq!(report["errors"][0]["kind"], "empty_input");
}
