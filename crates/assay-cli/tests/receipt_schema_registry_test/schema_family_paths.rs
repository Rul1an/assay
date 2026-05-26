use super::*;
#[test]
fn promptfoo_input_and_receipt_schemas_validate_supported_path() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("results.jsonl");
    let bundle = dir.path().join("promptfoo-receipts.tar.gz");
    fs::write(
        &input,
        concat!(
            r#"{"gradingResult":{"componentResults":[{"pass":true,"score":1,"reason":"Assertion passed","assertion":{"type":"equals","value":"Hello world"}}]}}"#,
            "\n",
            r#"{"gradingResult":{"componentResults":[{"pass":false,"score":0,"reason":"Expected output \"Goodbye world\" to equal \"Hello world\"","assertion":{"type":"equals","value":"Hello world"}}]}}"#,
            "\n"
        ),
    )
    .unwrap();

    for row in jsonl_values(&input) {
        assert_valid(
            "inputs/promptfoo-cli-jsonl-component-result.v1.schema.json",
            &row,
        );
    }

    Command::cargo_bin("assay")
        .unwrap()
        .arg("evidence")
        .arg("import")
        .arg("promptfoo-jsonl")
        .arg("--input")
        .arg(&input)
        .arg("--bundle-out")
        .arg(&bundle)
        .arg("--source-artifact-ref")
        .arg("results.jsonl")
        .arg("--run-id")
        .arg("promptfoo_schema_test")
        .arg("--import-time")
        .arg("2026-04-26T12:00:00Z")
        .assert()
        .success();

    for payload in bundle_payloads(&bundle) {
        assert_valid(
            "receipts/promptfoo.assertion-component.v1.schema.json",
            &payload,
        );
    }
}

#[test]
fn openfeature_input_and_receipt_schemas_validate_supported_path() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("openfeature-details.jsonl");
    let bundle = dir.path().join("openfeature-receipts.tar.gz");
    fs::write(
        &input,
        concat!(
            r#"{"schema":"openfeature.evaluation-details.export.v1","framework":"openfeature","surface":"evaluation_details","target_kind":"feature_flag","flag_key":"checkout.new_flow","result":{"value":true,"variant":"on","reason":"STATIC"}}"#,
            "\n",
            r#"{"schema":"openfeature.evaluation-details.export.v1","framework":"openfeature","surface":"evaluation_details","target_kind":"feature_flag","flag_key":"checkout.missing","result":{"value":false,"reason":"ERROR","error_code":"FLAG_NOT_FOUND"}}"#,
            "\n"
        ),
    )
    .unwrap();

    for row in jsonl_values(&input) {
        assert_valid(
            "inputs/openfeature-evaluation-details-export.v1.schema.json",
            &row,
        );
    }

    Command::cargo_bin("assay")
        .unwrap()
        .arg("evidence")
        .arg("import")
        .arg("openfeature-details")
        .arg("--input")
        .arg(&input)
        .arg("--bundle-out")
        .arg(&bundle)
        .arg("--source-artifact-ref")
        .arg("openfeature-details.jsonl")
        .arg("--run-id")
        .arg("openfeature_schema_test")
        .arg("--import-time")
        .arg("2026-04-27T12:00:00Z")
        .assert()
        .success();

    for payload in bundle_payloads(&bundle) {
        assert_valid(
            "receipts/openfeature.evaluation-details.v1.schema.json",
            &payload,
        );
    }
}

#[test]
fn cyclonedx_input_and_receipt_schemas_validate_supported_path() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("bom.cdx.json");
    let bundle = dir.path().join("cyclonedx-model-receipt.tar.gz");
    fs::write(
        &input,
        json!({
            "bomFormat": "CycloneDX",
            "specVersion": "1.7",
            "components": [
                {
                    "bom-ref": "pkg:huggingface/example/model@abc123",
                    "type": "machine-learning-model",
                    "publisher": "Example Inc.",
                    "name": "example-model",
                    "version": "1.0.0",
                    "purl": "pkg:huggingface/example/model@abc123",
                    "modelCard": {
                        "bom-ref": "model-card-example-model",
                        "modelParameters": {
                            "datasets": [{ "ref": "component-training-data" }]
                        }
                    }
                },
                {
                    "bom-ref": "component-training-data",
                    "type": "data",
                    "name": "Example Training Data"
                }
            ]
        })
        .to_string(),
    )
    .unwrap();

    let input_json: Value = serde_json::from_slice(&fs::read(&input).unwrap()).unwrap();
    assert_valid(
        "inputs/cyclonedx-mlbom-model-component-input.v1.schema.json",
        &input_json,
    );

    Command::cargo_bin("assay")
        .unwrap()
        .arg("evidence")
        .arg("import")
        .arg("cyclonedx-mlbom-model")
        .arg("--input")
        .arg(&input)
        .arg("--bundle-out")
        .arg(&bundle)
        .arg("--source-artifact-ref")
        .arg("bom.cdx.json")
        .arg("--run-id")
        .arg("cyclonedx_schema_test")
        .arg("--import-time")
        .arg("2026-04-28T12:00:00Z")
        .assert()
        .success();

    for payload in bundle_payloads(&bundle) {
        assert_valid(
            "receipts/cyclonedx.mlbom-model-component.v1.schema.json",
            &payload,
        );
    }
}

#[test]
fn mastra_input_and_receipt_schemas_validate_supported_path_without_claim_family() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("mastra-score-events.jsonl");
    let bundle = dir.path().join("mastra-score-receipts.tar.gz");
    fs::write(
        &input,
        concat!(
            r#"{"schema":"mastra.score-event.export.v1","framework":"mastra","surface":"observability.score_event","timestamp":"2026-04-30T10:31:38.858Z","score_id_ref":"f6605b31-af00-4b17-ae00-ed6262f4f411","scorer_id":"assay-scoreid-proof-scorer","score":0.91,"target_ref":"span:span-proof-001","trace_id_ref":"trace-proof-001","span_id_ref":"span-proof-001","score_trace_id_ref":"score-trace-proof-001","score_source":"live","metadata_ref":"metadata:scoreid-proof"}"#,
            "\n",
            r#"{"schema":"mastra.score-event.export.v1","framework":"mastra","surface":"observability.score_event","timestamp":"2026-04-15T18:58:12.297Z","scorer_name":"P14 Live Capture Scorer","score":0.18,"target_ref":"span:c4b7f4a58f2d90e1","trace_id_ref":"9f5bbab9073de1205f4a1de4925ad2b","span_id_ref":"c4b7f4a58f2d90e1","score_trace_id_ref":"score-trace:c4b7f4a58f2d90e1","score_source":"live","metadata_ref":"metadata:p14-live-capture"}"#,
            "\n"
        ),
    )
    .unwrap();

    for row in jsonl_values(&input) {
        assert_valid("inputs/mastra-score-event-export.v1.schema.json", &row);
    }

    Command::cargo_bin("assay")
        .unwrap()
        .arg("evidence")
        .arg("import")
        .arg("mastra-score-event")
        .arg("--input")
        .arg(&input)
        .arg("--bundle-out")
        .arg(&bundle)
        .arg("--source-artifact-ref")
        .arg("mastra-score-events.jsonl")
        .arg("--run-id")
        .arg("mastra_schema_test")
        .arg("--import-time")
        .arg("2026-04-28T12:00:00Z")
        .assert()
        .success();

    for payload in bundle_payloads(&bundle) {
        assert_valid("receipts/mastra.score-event.v1.schema.json", &payload);
    }
}

#[test]
fn pydantic_input_and_receipt_schemas_validate_supported_path_without_claim_family() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("pydantic-case-results.jsonl");
    let bundle = dir.path().join("pydantic-case-result-receipts.tar.gz");
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

    for row in jsonl_values(&input) {
        assert_valid("inputs/pydantic-case-result-export.v1.schema.json", &row);
    }

    Command::cargo_bin("assay")
        .unwrap()
        .arg("evidence")
        .arg("import")
        .arg("pydantic-case-result")
        .arg("--input")
        .arg(&input)
        .arg("--bundle-out")
        .arg(&bundle)
        .arg("--source-artifact-ref")
        .arg("pydantic-case-results.jsonl")
        .arg("--run-id")
        .arg("pydantic_schema_test")
        .arg("--import-time")
        .arg("2026-05-03T12:00:00Z")
        .assert()
        .success();

    for payload in bundle_payloads(&bundle) {
        assert_valid("receipts/pydantic.case-result.v1.schema.json", &payload);
    }
}

#[test]
fn livekit_input_and_receipt_schemas_validate_supported_path_without_claim_family() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("livekit-tool-action.json");
    let bundle = dir.path().join("livekit-tool-action-receipts.tar.gz");
    fs::write(
        &input,
        r#"{"schema":"livekit.function-tools-executed.export.v1","framework":"livekit_agents","surface":"function_tools_executed","runtime_mode":"agent_session","type":"function_tools_executed","event_ref":"turn-42:function_tools_executed:0","created_at":1778320801.5,"function_calls":[{"id":"item_call_lookup_order","call_id":"call_lookup_order_01","name":"lookup_customer_order","arguments":{"order_id":"ord_123","include_items":true},"created_at":1778320801.234,"group_id":null}],"function_call_outputs":[{"id":"item_output_lookup_order","call_id":"call_lookup_order_01","name":"lookup_customer_order","is_error":false,"output":{"status":"shipped","items_count":2},"created_at":1778320801.467}],"has_tool_reply":true,"has_agent_handoff":false}"#,
    )
    .unwrap();

    let input_json: Value = serde_json::from_slice(&fs::read(&input).unwrap()).unwrap();
    assert_valid(
        "inputs/livekit-function-tools-executed-export.v1.schema.json",
        &input_json,
    );

    Command::cargo_bin("assay")
        .unwrap()
        .arg("evidence")
        .arg("import")
        .arg("livekit-tool-action")
        .arg("--input")
        .arg(&input)
        .arg("--bundle-out")
        .arg(&bundle)
        .arg("--source-artifact-ref")
        .arg("livekit-tool-action.json")
        .arg("--run-id")
        .arg("livekit_schema_test")
        .arg("--import-time")
        .arg("2026-05-09T10:00:02Z")
        .assert()
        .success();

    for payload in bundle_payloads(&bundle) {
        assert_valid("receipts/livekit.tool-action.v1.schema.json", &payload);
    }
}
