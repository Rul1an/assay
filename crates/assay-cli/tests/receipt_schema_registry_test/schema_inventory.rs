use super::*;
#[test]
fn schema_cli_lists_receipt_and_input_schemas() {
    let output = assay_schema_command()
        .arg("list")
        .arg("--format")
        .arg("json")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(report["schema"], "assay.evidence.schema.list.v1");
    let schemas = report["schemas"].as_array().unwrap();
    assert_eq!(schemas.len(), 12);

    let names = schemas
        .iter()
        .map(|schema| schema["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(names.contains(&"promptfoo.assertion-component.v1"));
    assert!(names.contains(&"openfeature.evaluation-details.v1"));
    assert!(names.contains(&"cyclonedx.mlbom-model-component.v1"));
    assert!(names.contains(&"mastra.score-event.v1"));
    assert!(names.contains(&"pydantic.case-result.v1"));
    assert!(names.contains(&"livekit.tool-action.v1"));
    assert!(names.contains(&"promptfoo-cli-jsonl-component-result.v1"));
    assert!(names.contains(&"openfeature-evaluation-details-export.v1"));
    assert!(names.contains(&"cyclonedx-mlbom-model-component-input.v1"));
    assert!(names.contains(&"mastra-score-event-export.v1"));
    assert!(names.contains(&"pydantic-case-result-export.v1"));
    assert!(names.contains(&"livekit-function-tools-executed-export.v1"));

    for name in [
        "mastra.score-event.v1",
        "pydantic.case-result.v1",
        "livekit.tool-action.v1",
    ] {
        let schema = schemas
            .iter()
            .find(|schema| schema["name"] == name)
            .unwrap();
        assert_eq!(schema["importer_only"], true);
        assert!(schema["trust_basis_claim"].is_null());
    }
}

#[test]
fn receipt_family_matrix_keeps_mastra_score_receipts_importer_only() {
    let matrix = receipt_family_matrix();
    let mastra = matrix["importer_only_receipts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|family| family["event_type"] == "assay.receipt.mastra.score_event.v1")
        .expect("Mastra ScoreEvent receipt should stay in importer_only_receipts");

    assert_eq!(mastra["family"], "score_receipts");
    assert_eq!(mastra["source_system"], "mastra");
    assert_eq!(mastra["source_surface"], "observability.score_event");
    assert!(
        mastra["trust_basis_claim"].is_null(),
        "Mastra ScoreEvent receipts must remain importer-only until a later claim slice"
    );
    assert_eq!(
        mastra["claim_readiness_plan"],
        "../architecture/PLAN-P14D-MASTRA-SCORE-RECEIPT-TRUST-BASIS-READINESS-FREEZE-2026q2.md"
    );
}

#[test]
fn receipt_family_matrix_keeps_pydantic_case_result_receipts_importer_only() {
    let matrix = receipt_family_matrix();
    let pydantic = matrix["importer_only_receipts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|family| family["event_type"] == "assay.receipt.pydantic.case_result.v1")
        .expect("Pydantic case-result receipt should stay in importer_only_receipts");

    assert_eq!(pydantic["family"], "case_result_receipts");
    assert_eq!(pydantic["source_system"], "pydantic_evals");
    assert_eq!(
        pydantic["source_surface"],
        "evaluation_report.cases.case_result"
    );
    assert!(
        pydantic["trust_basis_claim"].is_null(),
        "Pydantic case-result receipts must remain importer-only until a later claim slice"
    );
    assert_eq!(
        pydantic["claim_readiness_plan"],
        "../architecture/PLAN-P9C-PYDANTIC-REDUCED-CASE-RESULT-RECEIPT-READINESS-2026q2.md"
    );
}

#[test]
fn schema_cli_paths_match_receipt_family_matrix() {
    let output = assay_schema_command()
        .arg("list")
        .arg("--format")
        .arg("json")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: Value = serde_json::from_slice(&output).unwrap();
    let cli_paths = report["schemas"]
        .as_array()
        .unwrap()
        .iter()
        .map(|schema| schema["source_path"].as_str().unwrap())
        .collect::<Vec<_>>();

    let matrix = receipt_family_matrix();
    let families = matrix["families"]
        .as_array()
        .unwrap()
        .iter()
        .chain(matrix["importer_only_receipts"].as_array().unwrap().iter());

    for family in families {
        for key in ["receipt_schema_path", "input_schema_path"] {
            let path = format!("docs/reference/{}", family[key].as_str().unwrap());
            assert!(
                cli_paths.contains(&path.as_str()),
                "schema CLI list should expose matrix path {path}"
            );
        }
    }
}

#[test]
fn packaged_schema_assets_match_docs_registry() {
    let output = assay_schema_command()
        .arg("list")
        .arg("--format")
        .arg("json")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: Value = serde_json::from_slice(&output).unwrap();

    for schema in report["schemas"].as_array().unwrap() {
        let relative = schema["source_path"]
            .as_str()
            .unwrap()
            .strip_prefix("docs/reference/receipt-schemas/")
            .unwrap();
        assert_eq!(
            fs::read(schema_path(relative)).unwrap(),
            fs::read(packaged_schema_path(relative)).unwrap(),
            "packaged schema asset should match docs registry for {relative}"
        );
    }
}

#[test]
fn schema_cli_shows_metadata_and_raw_schema() {
    let output = assay_schema_command()
        .arg("show")
        .arg("assay.receipt.promptfoo.assertion-component.v1")
        .arg("--format")
        .arg("json")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(report["schema"], "assay.evidence.schema.show.v1");
    assert_eq!(
        report["metadata"]["name"],
        "promptfoo.assertion-component.v1"
    );
    assert_eq!(report["metadata"]["kind"], "receipt");
    assert_eq!(
        report["metadata"]["trust_basis_claim"],
        "external_eval_receipt_boundary_visible"
    );

    let raw_output = assay_schema_command()
        .arg("show")
        .arg("promptfoo.assertion-component.v1")
        .arg("--raw")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let raw_schema: Value = serde_json::from_slice(&raw_output).unwrap();
    assert_eq!(
        raw_schema["$id"],
        "https://raw.githubusercontent.com/Rul1an/assay/v3.8.0/docs/reference/receipt-schemas/receipts/promptfoo.assertion-component.v1.schema.json"
    );
}
