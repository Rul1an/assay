use super::constants::{EVENT_SOURCE, EVENT_TYPE, SOURCE_SURFACE, SOURCE_SYSTEM};
use super::*;
use assay_evidence::bundle::BundleReader;
use std::fs;

#[test]
fn import_writes_verifiable_model_component_bundle_without_bodies() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("bom.cdx.json");
    let output = dir.path().join("cyclonedx-model.tar.gz");
    fs::write(&input, fixture_with_one_model()).unwrap();

    let code = cmd_cyclonedx_mlbom_model(CycloneDxMlBomModelArgs {
        input: input.clone(),
        bundle_out: output.clone(),
        bom_ref: None,
        source_artifact_ref: Some("bom.cdx.json".to_string()),
        run_id: "cyclonedx_test".to_string(),
        import_time: Some("2026-04-28T12:00:00Z".to_string()),
    })
    .unwrap();
    assert_eq!(code, exit_codes::OK);

    let reader = BundleReader::open(File::open(output).unwrap()).unwrap();
    assert_eq!(reader.manifest().event_count, 1);
    let events = reader.events().collect::<Result<Vec<_>>>().unwrap();
    let payload = &events[0].payload;
    assert_eq!(events[0].type_, EVENT_TYPE);
    assert_eq!(events[0].source, EVENT_SOURCE);
    assert_eq!(payload["source_system"], SOURCE_SYSTEM);
    assert_eq!(payload["source_surface"], SOURCE_SURFACE);
    assert_eq!(
        payload["model_component"]["bom_ref"],
        "pkg:huggingface/example/model@abc123"
    );
    assert_eq!(payload["model_component"]["name"], "example-model");
    assert_eq!(payload["model_component"]["version"], "1.0.0");
    assert_eq!(payload["model_component"]["publisher"], "Example Inc.");
    assert_eq!(
        payload["model_component"]["purl"],
        "pkg:huggingface/example/model@abc123"
    );
    assert_eq!(
        payload["model_component"]["dataset_refs"][0],
        "component-training-data"
    );
    assert_eq!(
        payload["model_component"]["model_card_refs"][0],
        "model-card-example-model"
    );

    let serialized = serde_json::to_string(payload).unwrap();
    assert!(!serialized.contains("quantitativeAnalysis"));
    assert!(!serialized.contains("ethicalConsiderations"));
    assert!(!serialized.contains("Speech Training Data"));
    assert!(!serialized.contains("licenses"));
    assert!(!serialized.contains("vulnerabilities"));
    assert!(!serialized.contains("pedigree"));
}

#[test]
fn import_requires_bom_ref_when_multiple_model_components_exist() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("bom.cdx.json");
    let output = dir.path().join("cyclonedx-model.tar.gz");
    fs::write(&input, fixture_with_two_models()).unwrap();

    let err = cmd_cyclonedx_mlbom_model(CycloneDxMlBomModelArgs {
        input,
        bundle_out: output,
        bom_ref: None,
        source_artifact_ref: None,
        run_id: "cyclonedx_test".to_string(),
        import_time: Some("2026-04-28T12:00:00Z".to_string()),
    })
    .unwrap_err();
    assert!(err
        .to_string()
        .contains("multiple machine-learning-model components"));
}

#[test]
fn import_selects_model_component_by_bom_ref() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("bom.cdx.json");
    let output = dir.path().join("cyclonedx-model.tar.gz");
    fs::write(&input, fixture_with_two_models()).unwrap();

    cmd_cyclonedx_mlbom_model(CycloneDxMlBomModelArgs {
        input,
        bundle_out: output.clone(),
        bom_ref: Some("component-secondary-model".to_string()),
        source_artifact_ref: None,
        run_id: "cyclonedx_test".to_string(),
        import_time: Some("2026-04-28T12:00:00Z".to_string()),
    })
    .unwrap();

    let reader = BundleReader::open(File::open(output).unwrap()).unwrap();
    let events = reader.events().collect::<Result<Vec<_>>>().unwrap();
    assert_eq!(
        events[0].payload["model_component"]["bom_ref"],
        "component-secondary-model"
    );
    assert_eq!(
        events[0].payload["model_component"]["name"],
        "secondary-model"
    );
}

#[test]
fn import_rejects_missing_or_non_model_bom_ref() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("bom.cdx.json");
    let output = dir.path().join("cyclonedx-model.tar.gz");
    fs::write(&input, fixture_with_one_model()).unwrap();

    let err = cmd_cyclonedx_mlbom_model(CycloneDxMlBomModelArgs {
        input,
        bundle_out: output,
        bom_ref: Some("component-training-data".to_string()),
        source_artifact_ref: None,
        run_id: "cyclonedx_test".to_string(),
        import_time: Some("2026-04-28T12:00:00Z".to_string()),
    })
    .unwrap_err();
    assert!(err
        .to_string()
        .contains("did not match a components[] machine-learning-model entry"));
}

#[test]
fn import_rejects_bom_without_model_components() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("bom.cdx.json");
    let output = dir.path().join("cyclonedx-model.tar.gz");
    fs::write(
        &input,
        r#"{"bomFormat":"CycloneDX","specVersion":"1.7","components":[{"bom-ref":"app","type":"application","name":"app"}]}"#,
    )
    .unwrap();

    let err = cmd_cyclonedx_mlbom_model(CycloneDxMlBomModelArgs {
        input,
        bundle_out: output,
        bom_ref: None,
        source_artifact_ref: None,
        run_id: "cyclonedx_test".to_string(),
        import_time: Some("2026-04-28T12:00:00Z".to_string()),
    })
    .unwrap_err();
    assert!(err.to_string().contains("no components[] entries"));
}

fn fixture_with_one_model() -> &'static str {
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
      "description": "This long description is intentionally not imported.",
      "pedigree": { "ancestors": [{ "name": "base-model" }] },
      "licenses": [{ "license": { "id": "Apache-2.0" } }],
      "modelCard": {
        "bom-ref": "model-card-example-model",
        "modelParameters": {
          "datasets": [{ "ref": "component-training-data" }]
        },
        "quantitativeAnalysis": {
          "performanceMetrics": [{ "type": "accuracy", "value": "0.9" }]
        },
        "considerations": {
          "ethicalConsiderations": [{ "name": "not imported" }]
        }
      }
    },
    {
      "bom-ref": "component-training-data",
      "type": "data",
      "publisher": "Example Inc.",
      "name": "Speech Training Data",
      "data": [{ "type": "dataset", "classification": "public" }]
    }
  ],
  "vulnerabilities": [{ "id": "CVE-0000-0000" }]
}"#
}

fn fixture_with_two_models() -> &'static str {
    r#"{
  "bomFormat": "CycloneDX",
  "specVersion": "1.7",
  "components": [
    {
      "bom-ref": "component-primary-model",
      "type": "machine-learning-model",
      "name": "primary-model"
    },
    {
      "bom-ref": "component-secondary-model",
      "type": "machine-learning-model",
      "name": "secondary-model"
    }
  ]
}"#
}
