use super::*;
#[test]
fn test_mastra_imported_score_receipts_verify_and_feed_trust_basis_generation() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("mastra-score-events.jsonl");
    let bundle = dir.path().join("mastra-score-receipts.tar.gz");
    fs::write(
        &input,
        concat!(
            r#"{"schema":"mastra.score-event.export.v1","framework":"mastra","surface":"observability.score_event","timestamp":"2026-04-30T10:31:38.858Z","score_id_ref":"f6605b31-af00-4b17-ae00-ed6262f4f411","scorer_id":"assay-scoreid-proof-scorer","score":0.91,"target_ref":"span:span-proof-001","trace_id_ref":"trace-proof-001","span_id_ref":"span-proof-001","score_trace_id_ref":"score-trace-proof-001","score_source":"live","metadata_ref":"metadata:scoreid-proof"}"#,
            "\n",
            r#"{"schema":"mastra.score-event.export.v1","framework":"mastra","surface":"observability.score_event","timestamp":"2026-04-15T18:58:12.297Z","scorer_name":"P14 Live Capture Scorer","score":0.18,"target_ref":"span:c4b7f4a58f2d90e1","trace_id_ref":"9f5bbab9073de1205f4a1de4925ad2b","span_id_ref":"c4b7f4a58f2d90e1","metadata_ref":"metadata:p14-live-capture"}"#,
            "\n"
        ),
    )
    .unwrap();

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
        .arg("mastra_trust_basis")
        .arg("--import-time")
        .arg("2026-04-28T12:00:00Z")
        .assert()
        .success();

    Command::cargo_bin("assay")
        .unwrap()
        .arg("evidence")
        .arg("verify")
        .arg(&bundle)
        .assert()
        .success();

    let output = Command::cargo_bin("assay")
        .unwrap()
        .arg("trust-basis")
        .arg("generate")
        .arg(&bundle)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let claims = json["claims"].as_array().unwrap();
    assert_eq!(claim(claims, "bundle_verified")["level"], "verified");
    assert_eq!(
        claim(claims, "external_eval_receipt_boundary_visible")["level"],
        "absent",
        "Mastra score receipts are not supported eval receipt claims in P14c"
    );
    assert_eq!(
        claim(claims, "external_decision_receipt_boundary_visible")["level"],
        "absent",
        "Mastra score receipts are not supported decision receipt claims"
    );
    assert_eq!(
        claim(claims, "external_inventory_receipt_boundary_visible")["level"],
        "absent",
        "Mastra score receipts are not inventory receipts"
    );
}

#[test]
fn test_pydantic_imported_case_result_receipts_verify_and_do_not_mutate_trust_basis_claims() {
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
        .arg("pydantic_trust_basis")
        .arg("--import-time")
        .arg("2026-05-03T12:00:00Z")
        .assert()
        .success();

    Command::cargo_bin("assay")
        .unwrap()
        .arg("evidence")
        .arg("verify")
        .arg(&bundle)
        .assert()
        .success();

    let output = Command::cargo_bin("assay")
        .unwrap()
        .arg("trust-basis")
        .arg("generate")
        .arg(&bundle)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let claims = json["claims"].as_array().unwrap();
    assert_eq!(claim(claims, "bundle_verified")["level"], "verified");
    assert_eq!(
        claim(claims, "external_eval_receipt_boundary_visible")["level"],
        "absent",
        "Pydantic case-result receipts are importer-only in P9d, not eval receipt claims"
    );
    assert_eq!(
        claim(claims, "external_decision_receipt_boundary_visible")["level"],
        "absent",
        "Pydantic case-result receipts are not decision receipts"
    );
    assert_eq!(
        claim(claims, "external_inventory_receipt_boundary_visible")["level"],
        "absent",
        "Pydantic case-result receipts are not inventory receipts"
    );
}

#[test]
fn test_livekit_imported_tool_action_receipts_verify_and_do_not_mutate_trust_basis_claims() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("livekit-tool-action.json");
    let bundle = dir.path().join("livekit-tool-action-receipts.tar.gz");
    fs::write(
        &input,
        r#"{"schema":"livekit.function-tools-executed.export.v1","framework":"livekit_agents","surface":"function_tools_executed","runtime_mode":"agent_session","type":"function_tools_executed","event_ref":"turn-42:function_tools_executed:0","created_at":1778320801.5,"function_calls":[{"id":"item_call_lookup_order","call_id":"call_lookup_order_01","name":"lookup_customer_order","arguments":{"order_id":"ord_123","include_items":true},"created_at":1778320801.234,"group_id":null}],"function_call_outputs":[{"id":"item_output_lookup_order","call_id":"call_lookup_order_01","name":"lookup_customer_order","is_error":false,"output":{"status":"shipped","items_count":2},"created_at":1778320801.467}],"has_tool_reply":true,"has_agent_handoff":false}"#,
    )
    .unwrap();

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
        .arg("livekit_trust_basis")
        .arg("--import-time")
        .arg("2026-05-09T10:00:02Z")
        .assert()
        .success();

    Command::cargo_bin("assay")
        .unwrap()
        .arg("evidence")
        .arg("verify")
        .arg(&bundle)
        .assert()
        .success();

    let output = Command::cargo_bin("assay")
        .unwrap()
        .arg("trust-basis")
        .arg("generate")
        .arg(&bundle)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let claims = json["claims"].as_array().unwrap();
    assert_eq!(claim(claims, "bundle_verified")["level"], "verified");
    assert_eq!(
        claim(claims, "external_eval_receipt_boundary_visible")["level"],
        "absent",
        "LiveKit tool-action receipts are acted-family candidates, not eval receipt claims"
    );
    assert_eq!(
        claim(claims, "external_decision_receipt_boundary_visible")["level"],
        "absent",
        "LiveKit tool-action receipts are not decision receipts"
    );
    assert_eq!(
        claim(claims, "external_inventory_receipt_boundary_visible")["level"],
        "absent",
        "LiveKit tool-action receipts are not inventory receipts"
    );
}

#[test]
fn test_cyclonedx_mlbom_model_receipts_verify_and_feed_trust_basis_generation() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("bom.cdx.json");
    let bundle = dir.path().join("cyclonedx-model-receipts.tar.gz");
    fs::write(
        &input,
        r#"{
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
      "name": "Training Data"
    }
  ]
}"#,
    )
    .unwrap();

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
        .arg("cyclonedx_trust_basis")
        .arg("--import-time")
        .arg("2026-04-28T12:00:00Z")
        .assert()
        .success();

    Command::cargo_bin("assay")
        .unwrap()
        .arg("evidence")
        .arg("verify")
        .arg(&bundle)
        .assert()
        .success();

    let output = Command::cargo_bin("assay")
        .unwrap()
        .arg("trust-basis")
        .arg("generate")
        .arg(&bundle)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let claims = json["claims"].as_array().unwrap();
    assert_eq!(
        claims.len(),
        10,
        "P45b keeps all frozen Trust Basis claims present"
    );
    assert_eq!(claim(claims, "bundle_verified")["level"], "verified");
    assert_eq!(
        claim(claims, "external_eval_receipt_boundary_visible")["level"],
        "absent",
        "CycloneDX ML-BOM model receipts are inventory receipts, not external eval receipts"
    );
    assert_eq!(
        claim(claims, "external_decision_receipt_boundary_visible")["level"],
        "absent",
        "CycloneDX ML-BOM model receipts are inventory receipts, not decision receipts"
    );
    assert_eq!(
        claim(claims, "external_inventory_receipt_boundary_visible")["level"],
        "verified",
        "CycloneDX ML-BOM model receipts should surface the bounded inventory receipt boundary claim"
    );
}

#[test]
fn test_cyclonedx_mlbom_formulation_fixture_stays_inventory_only() {
    let dir = tempdir().unwrap();
    let input = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../examples/cyclonedx-mlbom-model-component-evidence/fixtures/model-handoff-formulation.cdx.json",
    );
    let bundle = dir
        .path()
        .join("cyclonedx-model-formulation-receipts.tar.gz");

    assert!(
        input.exists(),
        "fixture should exist at {}",
        input.display()
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
        .arg("model-handoff-formulation.cdx.json")
        .arg("--bom-ref")
        .arg("pkg:huggingface/example/support-ticket-classifier@1.0.0")
        .arg("--run-id")
        .arg("cyclonedx_formulation_boundary")
        .arg("--import-time")
        .arg("2026-04-28T12:00:00Z")
        .assert()
        .success();

    Command::cargo_bin("assay")
        .unwrap()
        .arg("evidence")
        .arg("verify")
        .arg(&bundle)
        .assert()
        .success();

    let output = Command::cargo_bin("assay")
        .unwrap()
        .arg("trust-basis")
        .arg("generate")
        .arg(&bundle)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let claims = json["claims"].as_array().unwrap();
    assert_eq!(claim(claims, "bundle_verified")["level"], "verified");
    assert_eq!(
        claim(claims, "external_eval_receipt_boundary_visible")["level"],
        "absent",
        "CycloneDX formulation metrics are source-BOM context, not eval receipts"
    );
    assert_eq!(
        claim(claims, "external_decision_receipt_boundary_visible")["level"],
        "absent",
        "CycloneDX formulation handoff outputs are not decision receipts"
    );
    assert_eq!(
        claim(claims, "external_inventory_receipt_boundary_visible")["level"],
        "verified",
        "CycloneDX ML-BOM formulation fixture should remain an inventory receipt"
    );
}
