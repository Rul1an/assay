use crate::exit_codes;
use anyhow::{bail, Context, Result};
use assay_evidence::bundle::BundleWriter;
use assay_evidence::types::{EvidenceEvent, ProducerMeta};
use chrono::{DateTime, SecondsFormat, Utc};
use clap::Args;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

const EVENT_TYPE: &str = "assay.receipt.openfeature.evaluation_details.v1";
const EVENT_SOURCE: &str = "urn:assay:external:openfeature:evaluation-details";
const RECEIPT_SCHEMA: &str = "assay.receipt.openfeature.evaluation_details.v1";
const SOURCE_SYSTEM: &str = "openfeature";
const SOURCE_SURFACE: &str = "evaluation_details.boolean";
const REDUCER_VERSION: &str = "assay-openfeature-evaluation-details@0.1.0";
const INPUT_SCHEMA: &str = "openfeature.evaluation-details.export.v1";
const DEFAULT_RUN_ID: &str = "import-openfeature-details";
const MAX_FLAG_KEY_CHARS: usize = 200;
const MAX_BOUNDARY_STRING_CHARS: usize = 120;

#[derive(Debug, Args, Clone)]
pub struct OpenFeatureDetailsArgs {
    /// OpenFeature EvaluationDetails JSONL artifact file
    #[arg(long, value_name = "PATH")]
    pub input: PathBuf,

    /// Output Assay evidence bundle path (.tar.gz)
    #[arg(long, alias = "out", value_name = "PATH")]
    pub bundle_out: PathBuf,

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

pub fn cmd_openfeature_details(args: OpenFeatureDetailsArgs) -> Result<i32> {
    let import_time = parse_import_time(args.import_time.as_deref())?;
    let source_artifact_ref = args
        .source_artifact_ref
        .unwrap_or_else(|| default_source_artifact_ref(&args.input));
    // The receipt intentionally contains a narrow decision projection, but its
    // provenance binds back to the exact source artifact bytes.
    let source_artifact_digest = sha256_file(&args.input)
        .with_context(|| format!("failed to digest input {}", args.input.display()))?;
    let producer = ProducerMeta {
        name: "assay-cli".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        git: option_env!("ASSAY_GIT_SHA").map(str::to_string),
    };

    let events = read_openfeature_details_events(
        &args.input,
        &source_artifact_ref,
        &source_artifact_digest,
        &args.run_id,
        import_time,
        &producer,
    )?;

    let out_file = File::create(&args.bundle_out)
        .with_context(|| format!("failed to create bundle {}", args.bundle_out.display()))?;
    let mut writer = BundleWriter::new(out_file).with_producer(producer);
    for event in events {
        writer.add_event(event);
    }
    writer
        .finish()
        .with_context(|| format!("failed to write bundle {}", args.bundle_out.display()))?;

    eprintln!(
        "Imported OpenFeature EvaluationDetails decision receipts to {}",
        args.bundle_out.display()
    );

    Ok(exit_codes::OK)
}

fn read_openfeature_details_events(
    input: &Path,
    source_artifact_ref: &str,
    source_artifact_digest: &str,
    run_id: &str,
    import_time: DateTime<Utc>,
    producer: &ProducerMeta,
) -> Result<Vec<EvidenceEvent>> {
    if run_id.contains(':') {
        bail!("run_id cannot contain ':' because event ids use run_id:seq");
    }

    let file =
        File::open(input).with_context(|| format!("failed to open input {}", input.display()))?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();
    let mut saw_jsonl_row = false;

    for (line_index, line_result) in reader.lines().enumerate() {
        let line_number = line_index + 1;
        let line = line_result.with_context(|| format!("failed to read line {line_number}"))?;
        if line.trim().is_empty() {
            continue;
        }
        saw_jsonl_row = true;
        let row: Value = serde_json::from_str(&line)
            .with_context(|| format!("invalid JSONL object at line {line_number}"))?;
        let seq = events.len() as u64;
        let payload = reduce_evaluation_details(
            &row,
            source_artifact_ref,
            source_artifact_digest,
            import_time,
            line_number,
        )?;
        let event = EvidenceEvent::new(EVENT_TYPE, EVENT_SOURCE, run_id, seq, payload)
            .with_time(import_time)
            .with_producer(producer);
        events.push(event);
    }

    if !saw_jsonl_row {
        bail!("input contains no JSONL rows");
    }

    Ok(events)
}

fn reduce_evaluation_details(
    row: &Value,
    source_artifact_ref: &str,
    source_artifact_digest: &str,
    import_time: DateTime<Utc>,
    line_number: usize,
) -> Result<Value> {
    let record = row
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("line {line_number} must be a JSON object"))?;
    validate_top_level(record, line_number)?;

    let flag_key = bounded_string(
        record.get("flag_key"),
        "flag_key",
        MAX_FLAG_KEY_CHARS,
        line_number,
    )?;
    let result = record
        .get("result")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow::anyhow!("line {line_number} missing result object"))?;
    validate_result_keys(result, line_number)?;

    let value = result
        .get("value")
        .and_then(Value::as_bool)
        .ok_or_else(|| anyhow::anyhow!("line {line_number} result.value must be boolean"))?;

    let mut decision = Map::new();
    decision.insert("flag_key".to_string(), Value::String(flag_key));
    decision.insert(
        "value_type".to_string(),
        Value::String("boolean".to_string()),
    );
    decision.insert("value".to_string(), Value::Bool(value));
    if let Some(variant) = optional_bounded_string(
        result.get("variant"),
        "result.variant",
        MAX_BOUNDARY_STRING_CHARS,
        line_number,
    )? {
        decision.insert("variant".to_string(), Value::String(variant));
    }
    if let Some(reason) = optional_bounded_string(
        result.get("reason"),
        "result.reason",
        MAX_BOUNDARY_STRING_CHARS,
        line_number,
    )? {
        decision.insert("reason".to_string(), Value::String(reason));
    }
    if let Some(error_code) = optional_bounded_string(
        result.get("error_code"),
        "result.error_code",
        MAX_BOUNDARY_STRING_CHARS,
        line_number,
    )? {
        decision.insert("error_code".to_string(), Value::String(error_code));
    }

    Ok(json!({
        "schema": RECEIPT_SCHEMA,
        "source_system": SOURCE_SYSTEM,
        "source_surface": SOURCE_SURFACE,
        "source_artifact_ref": source_artifact_ref,
        "source_artifact_digest": source_artifact_digest,
        "reducer_version": REDUCER_VERSION,
        "imported_at": import_time.to_rfc3339_opts(SecondsFormat::Secs, true),
        "decision": Value::Object(decision),
    }))
}

fn validate_top_level(record: &Map<String, Value>, line_number: usize) -> Result<()> {
    let allowed = [
        "schema",
        "framework",
        "surface",
        "target_kind",
        "flag_key",
        "result",
    ];
    if let Some(key) = record.keys().find(|key| !allowed.contains(&key.as_str())) {
        bail!(
            "line {line_number} contains unsupported top-level key {key:?}; v1 excludes context, provider state, rules, and metadata"
        );
    }

    string_equals(record, "schema", INPUT_SCHEMA, line_number)?;
    string_equals(record, "framework", "openfeature", line_number)?;
    string_equals(record, "surface", "evaluation_details", line_number)?;
    string_equals(record, "target_kind", "feature_flag", line_number)?;
    Ok(())
}

fn validate_result_keys(result: &Map<String, Value>, line_number: usize) -> Result<()> {
    let allowed = ["value", "variant", "reason", "error_code"];
    if let Some(key) = result.keys().find(|key| !allowed.contains(&key.as_str())) {
        bail!(
            "line {line_number} contains unsupported result key {key:?}; v1 excludes error_message and metadata"
        );
    }
    if !result.contains_key("value") {
        bail!("line {line_number} missing result.value");
    }
    Ok(())
}

fn string_equals(
    record: &Map<String, Value>,
    key: &str,
    expected: &str,
    line_number: usize,
) -> Result<()> {
    match record.get(key).and_then(Value::as_str) {
        Some(actual) if actual == expected => Ok(()),
        Some(actual) => bail!("line {line_number} {key} must be {expected:?}, got {actual:?}"),
        None => bail!("line {line_number} missing string {key}"),
    }
}

fn bounded_string(
    value: Option<&Value>,
    field_name: &str,
    max_chars: usize,
    line_number: usize,
) -> Result<String> {
    let value = value
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("line {line_number} {field_name} must be a string"))?;
    validate_bounded_string(value, field_name, max_chars, line_number)
}

fn optional_bounded_string(
    value: Option<&Value>,
    field_name: &str,
    max_chars: usize,
    line_number: usize,
) -> Result<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    Ok(Some(validate_bounded_string(
        value.as_str().ok_or_else(|| {
            anyhow::anyhow!("line {line_number} {field_name} must be a string or null")
        })?,
        field_name,
        max_chars,
        line_number,
    )?))
}

fn validate_bounded_string(
    value: &str,
    field_name: &str,
    max_chars: usize,
    line_number: usize,
) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("line {line_number} {field_name} must not be empty");
    }
    if trimmed.chars().count() > max_chars {
        bail!("line {line_number} {field_name} must be at most {max_chars} characters");
    }
    if trimmed.contains('\n')
        || trimmed.contains('\r')
        || trimmed.contains('"')
        || trimmed.contains('`')
        || trimmed.contains('{')
        || trimmed.contains('}')
    {
        bail!("line {line_number} {field_name} is not reviewer-safe for v1");
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
        .unwrap_or("openfeature-details.jsonl")
        .to_string()
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
    fn import_writes_verifiable_boolean_decision_bundle() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("openfeature-details.jsonl");
        let output = dir.path().join("openfeature.tar.gz");
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

        let code = cmd_openfeature_details(OpenFeatureDetailsArgs {
            input: input.clone(),
            bundle_out: output.clone(),
            source_artifact_ref: Some("openfeature-details.jsonl".to_string()),
            run_id: "openfeature_test".to_string(),
            import_time: Some("2026-04-27T12:00:00Z".to_string()),
        })
        .unwrap();
        assert_eq!(code, exit_codes::OK);

        let reader = BundleReader::open(File::open(output).unwrap()).unwrap();
        assert_eq!(reader.manifest().event_count, 2);
        let events = reader.events().collect::<Result<Vec<_>>>().unwrap();
        assert_eq!(events[0].type_, EVENT_TYPE);
        assert_eq!(events[0].source, EVENT_SOURCE);
        assert_eq!(events[0].payload["source_surface"], SOURCE_SURFACE);
        assert_eq!(
            events[0].payload["decision"]["flag_key"],
            "checkout.new_flow"
        );
        assert_eq!(events[0].payload["decision"]["value_type"], "boolean");
        assert_eq!(events[0].payload["decision"]["value"], true);
        assert_eq!(events[0].payload["decision"]["variant"], "on");
        assert_eq!(events[0].payload["decision"]["reason"], "STATIC");
        assert_eq!(
            events[1].payload["decision"]["flag_key"],
            "checkout.missing"
        );
        assert_eq!(events[1].payload["decision"]["value"], false);
        assert_eq!(events[1].payload["decision"]["reason"], "ERROR");
        assert_eq!(
            events[1].payload["decision"]["error_code"],
            "FLAG_NOT_FOUND"
        );

        let serialized = serde_json::to_string(&events).unwrap();
        assert!(!serialized.contains("evaluation_context"));
        assert!(!serialized.contains("targeting_key"));
        assert!(!serialized.contains("flag_metadata"));
        assert!(!serialized.contains("provider_config"));
        assert!(!serialized.contains("error_message"));
    }

    #[test]
    fn import_rejects_non_boolean_value() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("openfeature-details.jsonl");
        let output = dir.path().join("openfeature.tar.gz");
        fs::write(
            &input,
            r#"{"schema":"openfeature.evaluation-details.export.v1","framework":"openfeature","surface":"evaluation_details","target_kind":"feature_flag","flag_key":"checkout.new_flow","result":{"value":"on","variant":"on","reason":"STATIC"}}"#,
        )
        .unwrap();

        let err = cmd_openfeature_details(OpenFeatureDetailsArgs {
            input,
            bundle_out: output,
            source_artifact_ref: None,
            run_id: "openfeature_test".to_string(),
            import_time: Some("2026-04-27T12:00:00Z".to_string()),
        })
        .unwrap_err();
        assert!(err.to_string().contains("result.value must be boolean"));
    }

    #[test]
    fn import_rejects_context_metadata_and_error_message() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("openfeature-details.jsonl");
        let output = dir.path().join("openfeature.tar.gz");
        fs::write(
            &input,
            r#"{"schema":"openfeature.evaluation-details.export.v1","framework":"openfeature","surface":"evaluation_details","target_kind":"feature_flag","flag_key":"checkout.new_flow","evaluation_context":{"targeting_key":"user-123"},"result":{"value":true,"error_message":"Flag leaked compared values"}}"#,
        )
        .unwrap();

        let err = cmd_openfeature_details(OpenFeatureDetailsArgs {
            input,
            bundle_out: output,
            source_artifact_ref: None,
            run_id: "openfeature_test".to_string(),
            import_time: Some("2026-04-27T12:00:00Z".to_string()),
        })
        .unwrap_err();
        assert!(err
            .to_string()
            .contains("unsupported top-level key \"evaluation_context\""));
    }

    #[test]
    fn import_rejects_error_message_even_without_context() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("openfeature-details.jsonl");
        let output = dir.path().join("openfeature.tar.gz");
        fs::write(
            &input,
            r#"{"schema":"openfeature.evaluation-details.export.v1","framework":"openfeature","surface":"evaluation_details","target_kind":"feature_flag","flag_key":"checkout.new_flow","result":{"value":true,"error_message":"message stays out of v1 receipts"}}"#,
        )
        .unwrap();

        let err = cmd_openfeature_details(OpenFeatureDetailsArgs {
            input,
            bundle_out: output,
            source_artifact_ref: None,
            run_id: "openfeature_test".to_string(),
            import_time: Some("2026-04-27T12:00:00Z".to_string()),
        })
        .unwrap_err();
        assert!(err
            .to_string()
            .contains("unsupported result key \"error_message\""));
    }
}
