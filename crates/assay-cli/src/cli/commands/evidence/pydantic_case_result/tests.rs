use super::constants::{EVENT_SOURCE, EVENT_TYPE, SOURCE_SURFACE};
use super::{cmd_pydantic_case_result, PydanticCaseResultArgs};
use crate::exit_codes;
use anyhow::Result;
use assay_evidence::bundle::BundleReader;
use std::fs::{self, File};

#[test]
fn import_writes_verifiable_case_result_bundle() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("pydantic-case-results.jsonl");
    let output = dir.path().join("pydantic-case-results.tar.gz");
    fs::write(
        &input,
        concat!(
            r#"{"schema":"pydantic-evals.report-case-result.export.v1","framework":"pydantic_evals","surface":"evaluation_report.cases.case_result","case_name":"case-hello","source_case_name":"source-hello","source_ref":"fixture:pydantic-case-results","results":[{"kind":"assertion","evaluator_name":"EqualsExpected","passed":true},{"kind":"score","evaluator_name":"ExactScorePoints","score":1.0,"reason":"maximum points"}],"timestamp":"2026-05-02T08:00:00Z"}"#,
            "\n",
            r#"{"schema":"pydantic-evals.report-case-result.export.v1","framework":"pydantic_evals","surface":"evaluation_report.cases.case_result","case_name":"case-bye","results":[{"kind":"assertion","evaluator_name":"EqualsExpected","passed":false},{"kind":"score","evaluator_name":"ExactScorePoints","score":0.25}],"timestamp":"2026-05-02T08:05:00Z"}"#,
            "\n"
        ),
    )
    .unwrap();

    let code = cmd_pydantic_case_result(PydanticCaseResultArgs {
        input: input.clone(),
        bundle_out: output.clone(),
        source_artifact_ref: Some("pydantic-case-results.jsonl".to_string()),
        run_id: "pydantic_test".to_string(),
        import_time: Some("2026-05-03T12:00:00Z".to_string()),
    })
    .unwrap();
    assert_eq!(code, exit_codes::OK);

    let reader = BundleReader::open(File::open(output).unwrap()).unwrap();
    assert_eq!(reader.manifest().event_count, 2);
    let events = reader.events().collect::<Result<Vec<_>>>().unwrap();
    assert_eq!(events[0].type_, EVENT_TYPE);
    assert_eq!(events[0].source, EVENT_SOURCE);
    assert_eq!(events[0].payload["source_surface"], SOURCE_SURFACE);
    assert_eq!(events[0].payload["case_result"]["case_name"], "case-hello");
    assert_eq!(
        events[0].payload["case_result"]["source_case_name"],
        "source-hello"
    );
    assert_eq!(
        events[0].payload["case_result"]["results"][0]["passed"],
        true
    );
    assert_eq!(events[0].payload["case_result"]["results"][1]["score"], 1.0);
    assert_eq!(
        events[0].payload["case_result"]["timestamp"],
        "2026-05-02T08:00:00.000Z"
    );

    let serialized = serde_json::to_string(&events).unwrap();
    assert!(!serialized.contains("expected_output"));
    assert!(!serialized.contains("\"output\""));
    assert!(!serialized.contains("trace_id"));
    assert!(!serialized.contains("span_id"));
    assert!(!serialized.contains("logfire"));
}

#[test]
fn import_rejects_raw_reportcase_fields() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("pydantic-case-results.jsonl");
    let output = dir.path().join("pydantic-case-results.tar.gz");
    fs::write(
        &input,
        r#"{"schema":"pydantic-evals.report-case-result.export.v1","framework":"pydantic_evals","surface":"evaluation_report.cases.case_result","case_name":"case-leaky","expected_output":"secret","output":"completion","results":[{"kind":"assertion","evaluator_name":"EqualsExpected","passed":true}],"timestamp":"2026-05-02T08:00:00Z"}"#,
    )
    .unwrap();

    let err = cmd_pydantic_case_result(PydanticCaseResultArgs {
        input,
        bundle_out: output,
        source_artifact_ref: None,
        run_id: "pydantic_test".to_string(),
        import_time: Some("2026-05-03T12:00:00Z".to_string()),
    })
    .unwrap_err();
    assert!(err
        .to_string()
        .contains("unsupported top-level key \"expected_output\""));
}

#[test]
fn import_rejects_non_boolean_assertion_value() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("pydantic-case-results.jsonl");
    let output = dir.path().join("pydantic-case-results.tar.gz");
    fs::write(
        &input,
        r#"{"schema":"pydantic-evals.report-case-result.export.v1","framework":"pydantic_evals","surface":"evaluation_report.cases.case_result","case_name":"case-hello","results":[{"kind":"assertion","evaluator_name":"EqualsExpected","passed":"true"}],"timestamp":"2026-05-02T08:00:00Z"}"#,
    )
    .unwrap();

    let err = cmd_pydantic_case_result(PydanticCaseResultArgs {
        input,
        bundle_out: output,
        source_artifact_ref: None,
        run_id: "pydantic_test".to_string(),
        import_time: Some("2026-05-03T12:00:00Z".to_string()),
    })
    .unwrap_err();
    assert!(err.to_string().contains("passed must be a boolean"));
}

#[test]
fn import_rejects_null_optional_fields() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("pydantic-case-results.jsonl");
    let output = dir.path().join("pydantic-case-results.tar.gz");
    fs::write(
        &input,
        r#"{"schema":"pydantic-evals.report-case-result.export.v1","framework":"pydantic_evals","surface":"evaluation_report.cases.case_result","case_name":"case-hello","source_ref":null,"results":[{"kind":"assertion","evaluator_name":"EqualsExpected","passed":true}],"timestamp":"2026-05-02T08:00:00Z"}"#,
    )
    .unwrap();

    let err = cmd_pydantic_case_result(PydanticCaseResultArgs {
        input,
        bundle_out: output,
        source_artifact_ref: None,
        run_id: "pydantic_test".to_string(),
        import_time: Some("2026-05-03T12:00:00Z".to_string()),
    })
    .unwrap_err();
    assert!(err
        .to_string()
        .contains("source_ref must be a string when present"));
}

#[test]
fn import_rejects_synthetic_case_id_ref() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("pydantic-case-results.jsonl");
    let output = dir.path().join("pydantic-case-results.tar.gz");
    fs::write(
        &input,
        r#"{"schema":"pydantic-evals.report-case-result.export.v1","framework":"pydantic_evals","surface":"evaluation_report.cases.case_result","case_name":"case-hello","case_id_ref":"case:synthetic","results":[{"kind":"assertion","evaluator_name":"EqualsExpected","passed":true}],"timestamp":"2026-05-02T08:00:00Z"}"#,
    )
    .unwrap();

    let err = cmd_pydantic_case_result(PydanticCaseResultArgs {
        input,
        bundle_out: output,
        source_artifact_ref: None,
        run_id: "pydantic_test".to_string(),
        import_time: Some("2026-05-03T12:00:00Z".to_string()),
    })
    .unwrap_err();
    assert!(err
        .to_string()
        .contains("unsupported top-level key \"case_id_ref\""));
}

#[test]
fn import_rejects_score_with_passed_field() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("pydantic-case-results.jsonl");
    let output = dir.path().join("pydantic-case-results.tar.gz");
    fs::write(
        &input,
        r#"{"schema":"pydantic-evals.report-case-result.export.v1","framework":"pydantic_evals","surface":"evaluation_report.cases.case_result","case_name":"case-hello","results":[{"kind":"score","evaluator_name":"ExactScorePoints","score":1.0,"passed":true}],"timestamp":"2026-05-02T08:00:00Z"}"#,
    )
    .unwrap();

    let err = cmd_pydantic_case_result(PydanticCaseResultArgs {
        input,
        bundle_out: output,
        source_artifact_ref: None,
        run_id: "pydantic_test".to_string(),
        import_time: Some("2026-05-03T12:00:00Z".to_string()),
    })
    .unwrap_err();
    assert!(err
        .to_string()
        .contains("score result must not include passed"));
}
