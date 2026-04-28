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

const EVENT_TYPE: &str = "assay.receipt.mastra.score_event.v1";
const EVENT_SOURCE: &str = "urn:assay:external:mastra:score-event";
const RECEIPT_SCHEMA: &str = "assay.receipt.mastra.score_event.v1";
const SOURCE_SYSTEM: &str = "mastra";
const SOURCE_SURFACE: &str = "observability.score_event";
const REDUCER_VERSION: &str = "assay-mastra-score-event@0.1.0";
const INPUT_SCHEMA: &str = "mastra.score-event.export.v1";
const DEFAULT_RUN_ID: &str = "import-mastra-score-event";
const MAX_BOUNDARY_STRING_CHARS: usize = 160;
const MAX_REASON_CHARS: usize = 240;

#[derive(Debug, Args, Clone)]
pub struct MastraScoreEventArgs {
    /// Mastra reduced ScoreEvent JSONL artifact file
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

pub fn cmd_mastra_score_event(args: MastraScoreEventArgs) -> Result<i32> {
    let import_time = parse_import_time(args.import_time.as_deref())?;
    let source_artifact_ref = args
        .source_artifact_ref
        .unwrap_or_else(|| default_source_artifact_ref(&args.input));
    // The receipt is a narrow score projection, but provenance binds back to
    // the exact reduced JSONL artifact bytes.
    let source_artifact_digest = sha256_file(&args.input)
        .with_context(|| format!("failed to digest input {}", args.input.display()))?;
    let producer = ProducerMeta {
        name: "assay-cli".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        git: option_env!("ASSAY_GIT_SHA").map(str::to_string),
    };

    let events = read_mastra_score_events(
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
        "Imported Mastra ScoreEvent receipts to {}",
        args.bundle_out.display()
    );

    Ok(exit_codes::OK)
}

fn read_mastra_score_events(
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
        let payload = reduce_score_event(
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

fn reduce_score_event(
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

    let timestamp = normalized_timestamp(record.get("timestamp"), "timestamp", line_number)?;
    let target_ref = bounded_string(
        record.get("target_ref"),
        "target_ref",
        MAX_BOUNDARY_STRING_CHARS,
        line_number,
    )?;
    let score = record
        .get("score")
        .and_then(Value::as_number)
        .ok_or_else(|| anyhow::anyhow!("line {line_number} score must be a number"))?
        .clone();

    let score_id_ref = optional_bounded_string(
        record.get("score_id_ref"),
        "score_id_ref",
        MAX_BOUNDARY_STRING_CHARS,
        line_number,
    )?;
    let scorer_id = optional_bounded_string(
        record.get("scorer_id"),
        "scorer_id",
        MAX_BOUNDARY_STRING_CHARS,
        line_number,
    )?;
    let scorer_name = optional_bounded_string(
        record.get("scorer_name"),
        "scorer_name",
        MAX_BOUNDARY_STRING_CHARS,
        line_number,
    )?;

    if scorer_id.is_none() && scorer_name.is_none() {
        bail!("line {line_number} missing scorer identity; expected scorer_id or scorer_name");
    }

    let mut score_event = Map::new();
    score_event.insert("score".to_string(), Value::Number(score));
    score_event.insert("target_ref".to_string(), Value::String(target_ref));
    score_event.insert("timestamp".to_string(), Value::String(timestamp));

    if let Some(score_id_ref) = score_id_ref {
        score_event.insert("score_id_ref".to_string(), Value::String(score_id_ref));
    }
    if let Some(scorer_id) = scorer_id {
        score_event.insert("scorer_id".to_string(), Value::String(scorer_id));
    }
    if let Some(scorer_name) = scorer_name {
        score_event.insert("scorer_name".to_string(), Value::String(scorer_name));
    }

    for field in [
        "scorer_version",
        "score_source",
        "trace_id_ref",
        "span_id_ref",
        "score_trace_id_ref",
        "target_entity_type",
        "metadata_ref",
    ] {
        if let Some(value) = optional_bounded_string(
            record.get(field),
            field,
            MAX_BOUNDARY_STRING_CHARS,
            line_number,
        )? {
            score_event.insert(field.to_string(), Value::String(value));
        }
    }

    if let Some(reason) = optional_bounded_string(
        record.get("reason"),
        "reason",
        MAX_REASON_CHARS,
        line_number,
    )? {
        score_event.insert("reason".to_string(), Value::String(reason));
    }

    Ok(json!({
        "schema": RECEIPT_SCHEMA,
        "source_system": SOURCE_SYSTEM,
        "source_surface": SOURCE_SURFACE,
        "source_artifact_ref": source_artifact_ref,
        "source_artifact_digest": source_artifact_digest,
        "reducer_version": REDUCER_VERSION,
        "imported_at": import_time.to_rfc3339_opts(SecondsFormat::Secs, true),
        "score_event": Value::Object(score_event),
    }))
}

fn validate_top_level(record: &Map<String, Value>, line_number: usize) -> Result<()> {
    let allowed = [
        "schema",
        "framework",
        "surface",
        "timestamp",
        "score",
        "target_ref",
        "score_id_ref",
        "scorer_id",
        "scorer_name",
        "scorer_version",
        "score_source",
        "reason",
        "trace_id_ref",
        "span_id_ref",
        "score_trace_id_ref",
        "target_entity_type",
        "metadata_ref",
    ];
    if let Some(key) = record.keys().find(|key| !allowed.contains(&key.as_str())) {
        bail!(
            "line {line_number} contains unsupported top-level key {key:?}; v1 imports reduced score-event artifacts and excludes raw metadata, correlationContext, traces, logs, metrics, and feedback"
        );
    }

    string_equals(record, "schema", INPUT_SCHEMA, line_number)?;
    string_equals(record, "framework", "mastra", line_number)?;
    string_equals(record, "surface", SOURCE_SURFACE, line_number)?;
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

fn normalized_timestamp(
    value: Option<&Value>,
    field_name: &str,
    line_number: usize,
) -> Result<String> {
    let value = value
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("line {line_number} {field_name} must be a string"))?;
    Ok(DateTime::parse_from_rfc3339(value)
        .with_context(|| format!("line {line_number} {field_name} must be RFC3339 with timezone"))?
        .with_timezone(&Utc)
        .to_rfc3339_opts(SecondsFormat::Millis, true))
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
        .unwrap_or("mastra-score-events.jsonl")
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
    fn import_writes_verifiable_score_event_bundle() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("mastra-score-events.jsonl");
        let output = dir.path().join("mastra-score-events.tar.gz");
        fs::write(
            &input,
            concat!(
                r#"{"schema":"mastra.score-event.export.v1","framework":"mastra","surface":"observability.score_event","timestamp":"2026-04-15T18:53:12.297Z","scorer_id":"p14-live-capture-scorer","score":0.92,"target_ref":"span:7c4180655970aca2","trace_id_ref":"59896b9a054b88cb48748463a0f2ab59","span_id_ref":"7c4180655970aca2","score_source":"live"}"#,
                "\n",
                r#"{"schema":"mastra.score-event.export.v1","framework":"mastra","surface":"observability.score_event","timestamp":"2026-04-15T18:58:12.297Z","scorer_name":"P14 Live Capture Scorer","score":0.18,"target_ref":"span:c4b7f4a58f2d90e1","trace_id_ref":"9f5bbab9073de1205f4a1de4925ad2b","span_id_ref":"c4b7f4a58f2d90e1","metadata_ref":"metadata:p14-live-capture"}"#,
                "\n"
            ),
        )
        .unwrap();

        let code = cmd_mastra_score_event(MastraScoreEventArgs {
            input: input.clone(),
            bundle_out: output.clone(),
            source_artifact_ref: Some("mastra-score-events.jsonl".to_string()),
            run_id: "mastra_test".to_string(),
            import_time: Some("2026-04-28T12:00:00Z".to_string()),
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
            events[0].payload["score_event"]["scorer_id"],
            "p14-live-capture-scorer"
        );
        assert_eq!(events[0].payload["score_event"]["score"], 0.92);
        assert_eq!(
            events[0].payload["score_event"]["timestamp"],
            "2026-04-15T18:53:12.297Z"
        );
        assert_eq!(
            events[1].payload["score_event"]["scorer_name"],
            "P14 Live Capture Scorer"
        );
        assert_eq!(
            events[1].payload["score_event"]["metadata_ref"],
            "metadata:p14-live-capture"
        );

        let serialized = serde_json::to_string(&events).unwrap();
        assert!(!serialized.contains("correlationContext"));
        assert!(!serialized.contains("\"metadata\":"));
        assert!(!serialized.contains("exportedSpan"));
        assert!(!serialized.contains("feedback"));
    }

    #[test]
    fn import_rejects_raw_metadata_and_correlation_context() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("mastra-score-events.jsonl");
        let output = dir.path().join("mastra-score-events.tar.gz");
        fs::write(
            &input,
            r#"{"schema":"mastra.score-event.export.v1","framework":"mastra","surface":"observability.score_event","timestamp":"2026-04-15T18:53:12.297Z","scorer_id":"p14-live-capture-scorer","score":0.92,"target_ref":"span:7c4180655970aca2","metadata":{"traceDepth":2},"correlationContext":{"entityType":"agent"}}"#,
        )
        .unwrap();

        let err = cmd_mastra_score_event(MastraScoreEventArgs {
            input,
            bundle_out: output,
            source_artifact_ref: None,
            run_id: "mastra_test".to_string(),
            import_time: Some("2026-04-28T12:00:00Z".to_string()),
        })
        .unwrap_err();
        assert!(err
            .to_string()
            .contains("unsupported top-level key \"metadata\""));
    }

    #[test]
    fn import_rejects_raw_callback_score_object() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("mastra-score-events.jsonl");
        let output = dir.path().join("mastra-score-events.tar.gz");
        fs::write(
            &input,
            r#"{"schema":"mastra.score-event.export.v1","framework":"mastra","surface":"observability.score_event","timestamp":"2026-04-15T18:53:12.297Z","scorer_id":"p14-live-capture-scorer","score":{"score":0.92},"target_ref":"span:7c4180655970aca2"}"#,
        )
        .unwrap();

        let err = cmd_mastra_score_event(MastraScoreEventArgs {
            input,
            bundle_out: output,
            source_artifact_ref: None,
            run_id: "mastra_test".to_string(),
            import_time: Some("2026-04-28T12:00:00Z".to_string()),
        })
        .unwrap_err();
        assert!(err.to_string().contains("score must be a number"));
    }

    #[test]
    fn import_rejects_missing_scorer_identity() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("mastra-score-events.jsonl");
        let output = dir.path().join("mastra-score-events.tar.gz");
        fs::write(
            &input,
            r#"{"schema":"mastra.score-event.export.v1","framework":"mastra","surface":"observability.score_event","timestamp":"2026-04-15T18:53:12.297Z","score":0.92,"target_ref":"span:7c4180655970aca2"}"#,
        )
        .unwrap();

        let err = cmd_mastra_score_event(MastraScoreEventArgs {
            input,
            bundle_out: output,
            source_artifact_ref: None,
            run_id: "mastra_test".to_string(),
            import_time: Some("2026-04-28T12:00:00Z".to_string()),
        })
        .unwrap_err();
        assert!(err.to_string().contains("missing scorer identity"));
    }

    #[test]
    fn import_rejects_legacy_underscore_surface() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("mastra-score-events.jsonl");
        let output = dir.path().join("mastra-score-events.tar.gz");
        fs::write(
            &input,
            r#"{"schema":"mastra.score-event.export.v1","framework":"mastra","surface":"observability_score_event","timestamp":"2026-04-15T18:53:12.297Z","scorer_id":"p14-live-capture-scorer","score":0.92,"target_ref":"span:7c4180655970aca2"}"#,
        )
        .unwrap();

        let err = cmd_mastra_score_event(MastraScoreEventArgs {
            input,
            bundle_out: output,
            source_artifact_ref: None,
            run_id: "mastra_test".to_string(),
            import_time: Some("2026-04-28T12:00:00Z".to_string()),
        })
        .unwrap_err();
        assert!(err
            .to_string()
            .contains("surface must be \"observability.score_event\""));
    }
}
