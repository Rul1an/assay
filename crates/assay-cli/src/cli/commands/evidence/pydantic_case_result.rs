use crate::exit_codes;
use anyhow::{bail, Context, Result};
use assay_evidence::bundle::BundleWriter;
use assay_evidence::types::{EvidenceEvent, ProducerMeta};
use chrono::{DateTime, SecondsFormat, Utc};
use clap::Args;
use serde_json::{json, Map, Number, Value};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

const EVENT_TYPE: &str = "assay.receipt.pydantic.case_result.v1";
const EVENT_SOURCE: &str = "urn:assay:external:pydantic-evals:case-result";
const RECEIPT_SCHEMA: &str = "assay.receipt.pydantic.case_result.v1";
const SOURCE_SYSTEM: &str = "pydantic_evals";
const SOURCE_SURFACE: &str = "evaluation_report.cases.case_result";
const REDUCER_VERSION: &str = "assay-pydantic-case-result@0.1.0";
const INPUT_SCHEMA: &str = "pydantic-evals.report-case-result.export.v1";
const DEFAULT_RUN_ID: &str = "import-pydantic-case-result";
const MAX_BOUNDARY_STRING_CHARS: usize = 160;
const MAX_REASON_CHARS: usize = 240;
const MAX_RESULTS: usize = 32;

#[derive(Debug, Args, Clone)]
pub struct PydanticCaseResultArgs {
    /// Pydantic Evals reduced case-result JSONL artifact file
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

pub fn cmd_pydantic_case_result(args: PydanticCaseResultArgs) -> Result<i32> {
    let import_time = parse_import_time(args.import_time.as_deref())?;
    let source_artifact_ref = args
        .source_artifact_ref
        .unwrap_or_else(|| default_source_artifact_ref(&args.input));
    let source_artifact_digest = sha256_file(&args.input)
        .with_context(|| format!("failed to digest input {}", args.input.display()))?;
    let producer = ProducerMeta {
        name: "assay-cli".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        git: option_env!("ASSAY_GIT_SHA").map(str::to_string),
    };

    let events = read_case_results(
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
        "Imported Pydantic Evals case-result receipts to {}",
        args.bundle_out.display()
    );

    Ok(exit_codes::OK)
}

fn read_case_results(
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
        let payload = reduce_case_result(
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

fn reduce_case_result(
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

    let case_name = bounded_string(
        record.get("case_name"),
        "case_name",
        MAX_BOUNDARY_STRING_CHARS,
        line_number,
    )?;
    let timestamp = normalized_timestamp(record.get("timestamp"), "timestamp", line_number)?;
    let results = reduced_results(record.get("results"), line_number)?;

    let mut case_result = Map::new();
    case_result.insert("case_name".to_string(), Value::String(case_name));
    case_result.insert("timestamp".to_string(), Value::String(timestamp));
    case_result.insert("results".to_string(), Value::Array(results));

    for field in ["source_case_name", "source_ref"] {
        if let Some(value) = optional_bounded_string(
            record.get(field),
            field,
            MAX_BOUNDARY_STRING_CHARS,
            line_number,
        )? {
            case_result.insert(field.to_string(), Value::String(value));
        }
    }

    Ok(json!({
        "schema": RECEIPT_SCHEMA,
        "source_system": SOURCE_SYSTEM,
        "source_surface": SOURCE_SURFACE,
        "source_artifact_ref": source_artifact_ref,
        "source_artifact_digest": source_artifact_digest,
        "reducer_version": REDUCER_VERSION,
        "imported_at": import_time.to_rfc3339_opts(SecondsFormat::Secs, true),
        "case_result": Value::Object(case_result),
    }))
}

fn reduced_results(value: Option<&Value>, line_number: usize) -> Result<Vec<Value>> {
    let values = value
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("line {line_number} results must be an array"))?;
    if values.is_empty() {
        bail!("line {line_number} results must not be empty");
    }
    if values.len() > MAX_RESULTS {
        bail!("line {line_number} results must contain at most {MAX_RESULTS} entries");
    }

    values
        .iter()
        .enumerate()
        .map(|(index, value)| reduced_result(value, line_number, index + 1))
        .collect()
}

fn reduced_result(value: &Value, line_number: usize, result_number: usize) -> Result<Value> {
    let result = value.as_object().ok_or_else(|| {
        anyhow::anyhow!("line {line_number} results[{result_number}] must be a JSON object")
    })?;
    validate_result_keys(result, line_number, result_number)?;

    let kind = bounded_string(
        result.get("kind"),
        "results[].kind",
        MAX_BOUNDARY_STRING_CHARS,
        line_number,
    )?;
    let evaluator_name = bounded_string(
        result.get("evaluator_name"),
        "results[].evaluator_name",
        MAX_BOUNDARY_STRING_CHARS,
        line_number,
    )?;

    let mut reduced = Map::new();
    reduced.insert("kind".to_string(), Value::String(kind.clone()));
    reduced.insert("evaluator_name".to_string(), Value::String(evaluator_name));

    match kind.as_str() {
        "assertion" => {
            if result.contains_key("score") {
                bail!(
                    "line {line_number} results[{result_number}] assertion result must not include score"
                );
            }
            let passed = result
                .get("passed")
                .and_then(Value::as_bool)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "line {line_number} results[{result_number}] assertion passed must be a boolean"
                    )
                })?;
            reduced.insert("passed".to_string(), Value::Bool(passed));
        }
        "score" => {
            if result.contains_key("passed") {
                bail!(
                    "line {line_number} results[{result_number}] score result must not include passed"
                );
            }
            let score = result
                .get("score")
                .and_then(Value::as_number)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "line {line_number} results[{result_number}] score must be a number"
                    )
                })?;
            reduced.insert("score".to_string(), Value::Number(normalize_number(score)?));
        }
        actual => bail!(
            "line {line_number} results[{result_number}] kind must be \"assertion\" or \"score\", got {actual:?}"
        ),
    }

    if let Some(reason) = optional_bounded_string(
        result.get("reason"),
        "results[].reason",
        MAX_REASON_CHARS,
        line_number,
    )? {
        reduced.insert("reason".to_string(), Value::String(reason));
    }

    Ok(Value::Object(reduced))
}

fn normalize_number(number: &Number) -> Result<Number> {
    if number
        .as_f64()
        .map(|value| value.is_finite())
        .unwrap_or(false)
    {
        return Ok(number.clone());
    }
    bail!("score must be finite");
}

fn validate_top_level(record: &Map<String, Value>, line_number: usize) -> Result<()> {
    let allowed = [
        "schema",
        "framework",
        "surface",
        "case_name",
        "source_case_name",
        "source_ref",
        "results",
        "timestamp",
    ];
    if let Some(key) = record.keys().find(|key| !allowed.contains(&key.as_str())) {
        bail!(
            "line {line_number} contains unsupported top-level key {key:?}; v1 imports reduced case-result artifacts and excludes raw ReportCase, trace, Logfire, prompt, completion, input, expected-output, and model-output fields"
        );
    }

    string_equals(record, "schema", INPUT_SCHEMA, line_number)?;
    string_equals(record, "framework", SOURCE_SYSTEM, line_number)?;
    string_equals(record, "surface", SOURCE_SURFACE, line_number)?;
    Ok(())
}

fn validate_result_keys(
    result: &Map<String, Value>,
    line_number: usize,
    result_number: usize,
) -> Result<()> {
    let allowed = ["kind", "evaluator_name", "passed", "score", "reason"];
    if let Some(key) = result.keys().find(|key| !allowed.contains(&key.as_str())) {
        bail!(
            "line {line_number} results[{result_number}] contains unsupported key {key:?}; v1 keeps only bounded evaluator identity and assertion/score values"
        );
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
    Ok(Some(validate_bounded_string(
        value.as_str().ok_or_else(|| {
            anyhow::anyhow!("line {line_number} {field_name} must be a string when present")
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
        .unwrap_or("pydantic-case-results.jsonl")
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
}
