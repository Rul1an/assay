use crate::exit_codes;
use anyhow::{bail, Context, Result};
use assay_evidence::bundle::BundleWriter;
use assay_evidence::types::{EvidenceEvent, ProducerMeta};
use chrono::{DateTime, SecondsFormat, Utc};
use clap::Args;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

const EVENT_TYPE: &str = "assay.receipt.cyclonedx.mlbom_model_component.v1";
const EVENT_SOURCE: &str = "urn:assay:external:cyclonedx:mlbom-model-component";
const RECEIPT_SCHEMA: &str = "assay.receipt.cyclonedx.mlbom-model-component.v1";
const SOURCE_SYSTEM: &str = "cyclonedx";
const SOURCE_SURFACE: &str = "bom.components[type=machine-learning-model]";
const REDUCER_VERSION: &str = "assay-cyclonedx-mlbom-model-component@0.1.0";
const DEFAULT_RUN_ID: &str = "import-cyclonedx-mlbom-model";
const MAX_BOUNDARY_STRING_CHARS: usize = 240;
const MAX_REF_COUNT: usize = 32;

#[derive(Debug, Args, Clone)]
pub struct CycloneDxMlBomModelArgs {
    /// CycloneDX JSON BOM artifact file
    #[arg(long, value_name = "PATH")]
    pub input: PathBuf,

    /// Output Assay evidence bundle path (.tar.gz)
    #[arg(long, alias = "out", value_name = "PATH")]
    pub bundle_out: PathBuf,

    /// Select a machine-learning-model component by bom-ref
    #[arg(long)]
    pub bom_ref: Option<String>,

    /// Reviewer-safe source artifact reference stored in receipts
    #[arg(long)]
    pub source_artifact_ref: Option<String>,

    /// Assay import run id used for receipt provenance and event ids
    #[arg(long, default_value = DEFAULT_RUN_ID)]
    pub run_id: String,

    /// Import timestamp for deterministic fixtures (RFC3339 UTC recommended)
    #[arg(long)]
    pub import_time: Option<String>,
}

pub fn cmd_cyclonedx_mlbom_model(args: CycloneDxMlBomModelArgs) -> Result<i32> {
    let import_time = parse_import_time(args.import_time.as_deref())?;
    let source_artifact_ref = args
        .source_artifact_ref
        .unwrap_or_else(|| default_source_artifact_ref(&args.input));
    // The receipt reduces one selected model component, while provenance binds
    // back to the exact BOM bytes that carried the richer inventory.
    let source_artifact_digest = sha256_file(&args.input)
        .with_context(|| format!("failed to digest input {}", args.input.display()))?;
    let producer = ProducerMeta {
        name: "assay-cli".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        git: option_env!("ASSAY_GIT_SHA").map(str::to_string),
    };

    let event = read_cyclonedx_model_event(
        &args.input,
        args.bom_ref.as_deref(),
        &source_artifact_ref,
        &source_artifact_digest,
        &args.run_id,
        import_time,
        &producer,
    )?;

    let out_file = File::create(&args.bundle_out)
        .with_context(|| format!("failed to create bundle {}", args.bundle_out.display()))?;
    let mut writer = BundleWriter::new(out_file).with_producer(producer);
    writer.add_event(event);
    writer
        .finish()
        .with_context(|| format!("failed to write bundle {}", args.bundle_out.display()))?;

    eprintln!(
        "Imported CycloneDX ML-BOM model component receipt to {}",
        args.bundle_out.display()
    );

    Ok(exit_codes::OK)
}

fn read_cyclonedx_model_event(
    input: &Path,
    bom_ref: Option<&str>,
    source_artifact_ref: &str,
    source_artifact_digest: &str,
    run_id: &str,
    import_time: DateTime<Utc>,
    producer: &ProducerMeta,
) -> Result<EvidenceEvent> {
    if run_id.contains(':') {
        bail!("run_id cannot contain ':' because event ids use run_id:seq");
    }

    let bom = read_json_file(input)?;
    let payload = reduce_model_component(
        &bom,
        bom_ref,
        source_artifact_ref,
        source_artifact_digest,
        import_time,
    )?;

    Ok(
        EvidenceEvent::new(EVENT_TYPE, EVENT_SOURCE, run_id, 0, payload)
            .with_time(import_time)
            .with_producer(producer),
    )
}

fn reduce_model_component(
    bom: &Value,
    bom_ref: Option<&str>,
    source_artifact_ref: &str,
    source_artifact_digest: &str,
    import_time: DateTime<Utc>,
) -> Result<Value> {
    let bom_obj = bom
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("CycloneDX input must be a JSON object"))?;
    string_equals(bom_obj, "bomFormat", "CycloneDX")?;

    let components = bom_obj
        .get("components")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("CycloneDX input must contain components[]"))?;

    let candidates = components
        .iter()
        .filter(|component| {
            component
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|value| value == "machine-learning-model")
        })
        .collect::<Vec<_>>();

    if candidates.is_empty() {
        bail!("CycloneDX input contains no components[] entries with type machine-learning-model");
    }

    let selected = select_component(&candidates, bom_ref)?;
    let selected_obj = selected
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("selected CycloneDX component must be an object"))?;

    let bom_ref = bounded_string(
        selected_obj.get("bom-ref"),
        "component.bom-ref",
        MAX_BOUNDARY_STRING_CHARS,
    )?;
    let name = bounded_string(
        selected_obj.get("name"),
        "component.name",
        MAX_BOUNDARY_STRING_CHARS,
    )?;

    let mut model_component = Map::new();
    model_component.insert("bom_ref".to_string(), Value::String(bom_ref));
    model_component.insert("name".to_string(), Value::String(name));
    insert_optional_string(
        &mut model_component,
        selected_obj,
        "version",
        "component.version",
    )?;
    insert_optional_string(
        &mut model_component,
        selected_obj,
        "publisher",
        "component.publisher",
    )?;
    insert_optional_string(&mut model_component, selected_obj, "purl", "component.purl")?;

    let dataset_refs = dataset_refs(selected_obj)?;
    if !dataset_refs.is_empty() {
        model_component.insert("dataset_refs".to_string(), strings_value(dataset_refs));
    }
    let model_card_refs = model_card_refs(selected_obj)?;
    if !model_card_refs.is_empty() {
        model_component.insert(
            "model_card_refs".to_string(),
            strings_value(model_card_refs),
        );
    }

    Ok(json!({
        "schema": RECEIPT_SCHEMA,
        "source_system": SOURCE_SYSTEM,
        "source_surface": SOURCE_SURFACE,
        "source_artifact_ref": source_artifact_ref,
        "source_artifact_digest": source_artifact_digest,
        "reducer_version": REDUCER_VERSION,
        "imported_at": import_time.to_rfc3339_opts(SecondsFormat::Secs, true),
        "model_component": Value::Object(model_component),
    }))
}

fn select_component<'a>(candidates: &'a [&'a Value], bom_ref: Option<&str>) -> Result<&'a Value> {
    if let Some(bom_ref) = bom_ref {
        let matches = candidates
            .iter()
            .copied()
            .filter(|component| {
                component
                    .get("bom-ref")
                    .and_then(Value::as_str)
                    .is_some_and(|actual| actual == bom_ref)
            })
            .collect::<Vec<_>>();
        return match matches.as_slice() {
            [selected] => Ok(*selected),
            [] => bail!(
                "--bom-ref {bom_ref:?} did not match a components[] machine-learning-model entry"
            ),
            _ => bail!("--bom-ref {bom_ref:?} matched multiple model components"),
        };
    }

    match candidates {
        [selected] => Ok(*selected),
        _ => bail!(
            "CycloneDX input contains multiple machine-learning-model components; pass --bom-ref to select one"
        ),
    }
}

fn dataset_refs(component: &Map<String, Value>) -> Result<Vec<String>> {
    let mut refs = Vec::new();
    let Some(datasets) = component
        .get("modelCard")
        .and_then(|model_card| model_card.get("modelParameters"))
        .and_then(|model_parameters| model_parameters.get("datasets"))
        .and_then(Value::as_array)
    else {
        return Ok(refs);
    };

    for (index, dataset) in datasets.iter().enumerate() {
        let Some(reference) = dataset.get("ref").and_then(Value::as_str) else {
            continue;
        };
        refs.push(validate_bounded_string(
            reference,
            &format!("modelCard.modelParameters.datasets[{index}].ref"),
            MAX_BOUNDARY_STRING_CHARS,
        )?);
        if refs.len() > MAX_REF_COUNT {
            bail!("modelCard.modelParameters.datasets has more than {MAX_REF_COUNT} refs");
        }
    }

    Ok(refs)
}

fn model_card_refs(component: &Map<String, Value>) -> Result<Vec<String>> {
    let mut refs = Vec::new();
    if let Some(reference) = component
        .get("modelCard")
        .and_then(|model_card| model_card.get("bom-ref"))
        .and_then(Value::as_str)
    {
        refs.push(validate_bounded_string(
            reference,
            "modelCard.bom-ref",
            MAX_BOUNDARY_STRING_CHARS,
        )?);
    }
    Ok(refs)
}

fn insert_optional_string(
    output: &mut Map<String, Value>,
    component: &Map<String, Value>,
    key: &str,
    field_name: &str,
) -> Result<()> {
    if let Some(value) =
        optional_bounded_string(component.get(key), field_name, MAX_BOUNDARY_STRING_CHARS)?
    {
        output.insert(key.to_string(), Value::String(value));
    }
    Ok(())
}

fn strings_value(values: Vec<String>) -> Value {
    Value::Array(values.into_iter().map(Value::String).collect())
}

fn string_equals(record: &Map<String, Value>, key: &str, expected: &str) -> Result<()> {
    match record.get(key).and_then(Value::as_str) {
        Some(actual) if actual == expected => Ok(()),
        Some(actual) => bail!("{key} must be {expected:?}, got {actual:?}"),
        None => bail!("missing string {key}"),
    }
}

fn bounded_string(value: Option<&Value>, field_name: &str, max_chars: usize) -> Result<String> {
    let value = value
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("{field_name} must be a string"))?;
    validate_bounded_string(value, field_name, max_chars)
}

fn optional_bounded_string(
    value: Option<&Value>,
    field_name: &str,
    max_chars: usize,
) -> Result<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    Ok(Some(validate_bounded_string(
        value
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("{field_name} must be a string or null"))?,
        field_name,
        max_chars,
    )?))
}

fn validate_bounded_string(value: &str, field_name: &str, max_chars: usize) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("{field_name} must not be empty");
    }
    if trimmed.chars().count() > max_chars {
        bail!("{field_name} must be at most {max_chars} characters");
    }
    if trimmed.contains('\n')
        || trimmed.contains('\r')
        || trimmed.contains('"')
        || trimmed.contains('`')
        || trimmed.contains('{')
        || trimmed.contains('}')
    {
        bail!("{field_name} is not reviewer-safe for v1");
    }
    Ok(trimmed.to_string())
}

fn parse_import_time(value: Option<&str>) -> Result<DateTime<Utc>> {
    match value {
        Some(value) => Ok(DateTime::parse_from_rfc3339(value)
            .with_context(|| format!("invalid --import-time {value:?}; expected RFC3339"))?
            .with_timezone(&Utc)),
        None => Ok(Utc::now()),
    }
}

fn default_source_artifact_ref(input: &Path) -> String {
    input
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("bom.cdx.json")
        .to_string()
}

fn read_json_file(path: &Path) -> Result<Value> {
    let file =
        File::open(path).with_context(|| format!("failed to open input {}", path.display()))?;
    serde_json::from_reader(BufReader::new(file))
        .with_context(|| format!("invalid JSON input {}", path.display()))
}

fn sha256_file(path: &Path) -> Result<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("sha256:{}", hex::encode(hasher.finalize())))
}

#[cfg(test)]
mod tests {
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
}
