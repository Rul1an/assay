use assay_evidence::bundle::BundleReader;
use assert_cmd::Command;
use jsonschema::{Draft, Validator};
use serde_json::{json, Value};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use tempfile::tempdir;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn schema_path(relative: &str) -> PathBuf {
    repo_root()
        .join("docs/reference/receipt-schemas")
        .join(relative)
}

fn packaged_schema_path(relative: &str) -> PathBuf {
    repo_root()
        .join("crates/assay-cli/receipt-schemas")
        .join(relative)
}

fn receipt_family_matrix() -> Value {
    let path = repo_root().join("docs/reference/receipt-family-matrix.json");
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn compile_schema(relative: &str) -> Validator {
    let path = schema_path(relative);
    let schema: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    jsonschema::options()
        .with_draft(Draft::Draft202012)
        .build(&schema)
        .unwrap_or_else(|err| panic!("failed to compile {}: {err}", path.display()))
}

fn assert_valid(schema_relative: &str, instance: &Value) {
    let validator = compile_schema(schema_relative);
    if validator.is_valid(instance) {
        return;
    }
    let errors = validator
        .iter_errors(instance)
        .map(|err| format!("{err} at {}", err.instance_path()))
        .collect::<Vec<_>>()
        .join("\n");
    panic!("{schema_relative} validation failed:\n{errors}\ninstance: {instance}");
}

fn assay_schema_command() -> Command {
    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.arg("evidence").arg("schema");
    cmd
}

fn jsonl_values(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn bundle_payloads(path: &Path) -> Vec<Value> {
    let reader = BundleReader::open(File::open(path).unwrap()).unwrap();
    reader
        .events()
        .map(|event| event.unwrap().payload)
        .collect()
}

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
    assert_eq!(schemas.len(), 10);

    let names = schemas
        .iter()
        .map(|schema| schema["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(names.contains(&"promptfoo.assertion-component.v1"));
    assert!(names.contains(&"openfeature.evaluation-details.v1"));
    assert!(names.contains(&"cyclonedx.mlbom-model-component.v1"));
    assert!(names.contains(&"mastra.score-event.v1"));
    assert!(names.contains(&"pydantic.case-result.v1"));
    assert!(names.contains(&"promptfoo-cli-jsonl-component-result.v1"));
    assert!(names.contains(&"openfeature-evaluation-details-export.v1"));
    assert!(names.contains(&"cyclonedx-mlbom-model-component-input.v1"));
    assert!(names.contains(&"mastra-score-event-export.v1"));
    assert!(names.contains(&"pydantic-case-result-export.v1"));

    for name in ["mastra.score-event.v1", "pydantic.case-result.v1"] {
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
    let matrix = receipt_family_matrix();
    let families = matrix["families"]
        .as_array()
        .unwrap()
        .iter()
        .chain(matrix["importer_only_receipts"].as_array().unwrap().iter());

    for family in families {
        for key in ["receipt_schema_path", "input_schema_path"] {
            let relative = family[key]
                .as_str()
                .unwrap()
                .strip_prefix("receipt-schemas/")
                .unwrap();
            assert_eq!(
                fs::read(schema_path(relative)).unwrap(),
                fs::read(packaged_schema_path(relative)).unwrap(),
                "packaged schema asset should match docs registry for {relative}"
            );
        }
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
