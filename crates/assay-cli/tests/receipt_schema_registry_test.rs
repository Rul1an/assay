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
            r#"{"schema":"mastra.score-event.export.v1","framework":"mastra","surface":"observability.score_event","timestamp":"2026-04-15T18:53:12.297Z","scorer_id":"p14-live-capture-scorer","score":0.92,"target_ref":"span:7c4180655970aca2","trace_id_ref":"59896b9a054b88cb48748463a0f2ab59","span_id_ref":"7c4180655970aca2","score_trace_id_ref":"score-trace:7c4180655970aca2","score_source":"live"}"#,
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
