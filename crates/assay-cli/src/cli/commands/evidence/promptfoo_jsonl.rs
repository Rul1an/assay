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

const EVENT_TYPE: &str = "assay.receipt.promptfoo.assertion_component.v1";
const EVENT_SOURCE: &str = "urn:assay:external:promptfoo:assertion-component";
const RECEIPT_SCHEMA: &str = "assay.receipt.promptfoo.assertion-component.v1";
const SOURCE_SYSTEM: &str = "promptfoo";
const SOURCE_SURFACE: &str = "cli-jsonl.gradingResult.componentResults";
const REDUCER_VERSION: &str = "assay-promptfoo-jsonl-component-result@0.1.0";
const DEFAULT_RUN_ID: &str = "import-promptfoo-jsonl";
const MAX_REASON_CHARS: usize = 160;

#[derive(Debug, Args, Clone)]
pub struct PromptfooJsonlArgs {
    /// Promptfoo CLI JSONL output file
    #[arg(long, value_name = "PATH")]
    pub input: PathBuf,

    /// Output Assay evidence bundle path (.tar.gz)
    #[arg(long, alias = "out", value_name = "PATH")]
    pub bundle_out: PathBuf,

    /// Reviewer-safe source artifact reference stored in receipts
    #[arg(long)]
    pub source_artifact_ref: Option<String>,

    /// Assay import run id used for receipt event ids
    #[arg(long, default_value = DEFAULT_RUN_ID)]
    pub run_id: String,

    /// Import timestamp for deterministic fixtures (RFC3339 UTC recommended)
    #[arg(long)]
    pub import_time: Option<String>,
}

pub fn cmd_promptfoo_jsonl(args: PromptfooJsonlArgs) -> Result<i32> {
    let import_time = parse_import_time(args.import_time.as_deref())?;
    let source_artifact_ref = args
        .source_artifact_ref
        .unwrap_or_else(|| default_source_artifact_ref(&args.input));
    // Deliberately digest the full source artifact bytes before parsing. The
    // receipt provenance binds to the exact JSONL artifact, independent of the
    // reduced component payloads we choose to import.
    let source_artifact_digest = sha256_file(&args.input)
        .with_context(|| format!("failed to digest input {}", args.input.display()))?;
    let producer = ProducerMeta {
        name: "assay-cli".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        git: option_env!("ASSAY_GIT_SHA").map(str::to_string),
    };

    let events = read_promptfoo_jsonl_events(
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
        "Imported Promptfoo assertion component receipts to {}",
        args.bundle_out.display()
    );

    Ok(exit_codes::OK)
}

fn read_promptfoo_jsonl_events(
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
        let components = row
            .pointer("/gradingResult/componentResults")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                anyhow::anyhow!("line {line_number} is missing gradingResult.componentResults[]")
            })?;
        if components.is_empty() {
            bail!("line {line_number} has empty gradingResult.componentResults[]");
        }

        for (component_index, component) in components.iter().enumerate() {
            let seq = events.len() as u64;
            let payload = reduce_component_result(
                &row,
                component,
                component_index,
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
    }

    if !saw_jsonl_row {
        bail!("input contains no JSONL rows");
    }

    Ok(events)
}

fn reduce_component_result(
    row: &Value,
    component: &Value,
    component_index: usize,
    source_artifact_ref: &str,
    source_artifact_digest: &str,
    import_time: DateTime<Utc>,
    line_number: usize,
) -> Result<Value> {
    let pass = component
        .get("pass")
        .and_then(Value::as_bool)
        .ok_or_else(|| {
            anyhow::anyhow!("line {line_number} component {component_index} missing boolean pass")
        })?;
    let score = binary_score(component.get("score"), line_number, component_index)?;
    let assertion_type = assertion_type(row, component, component_index, line_number)?;
    if assertion_type != "equals" {
        bail!(
            "line {line_number} component {component_index} has unsupported assertion type {assertion_type:?}; v1 supports only equals"
        );
    }

    let mut result = Map::new();
    result.insert("pass".to_string(), Value::Bool(pass));
    result.insert("score".to_string(), Value::Number(score.into()));
    if let Some(reason) = safe_reason(component.get("reason"), pass) {
        result.insert("reason".to_string(), Value::String(reason));
    }

    Ok(json!({
        "schema": RECEIPT_SCHEMA,
        "source_system": SOURCE_SYSTEM,
        "source_surface": SOURCE_SURFACE,
        "source_artifact_ref": source_artifact_ref,
        "source_artifact_digest": source_artifact_digest,
        "reducer_version": REDUCER_VERSION,
        "imported_at": import_time.to_rfc3339_opts(SecondsFormat::Secs, true),
        "assertion_type": assertion_type,
        "result": Value::Object(result),
    }))
}

fn assertion_type<'a>(
    row: &'a Value,
    component: &'a Value,
    component_index: usize,
    line_number: usize,
) -> Result<&'a str> {
    if let Some(value) = component
        .pointer("/assertion/type")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        return Ok(value);
    }

    row.pointer("/testCase/assert")
        .and_then(Value::as_array)
        .and_then(|assertions| assertions.get(component_index))
        .and_then(|assertion| assertion.get("type"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "line {line_number} component {component_index} does not expose an assertion type"
            )
        })
}

fn binary_score(value: Option<&Value>, line_number: usize, component_index: usize) -> Result<i64> {
    let value = value.ok_or_else(|| {
        anyhow::anyhow!("line {line_number} component {component_index} missing integer score")
    })?;
    let score = value.as_i64().ok_or_else(|| {
        anyhow::anyhow!(
            "line {line_number} component {component_index} score must be integer 0 or 1"
        )
    })?;
    match score {
        0 | 1 => Ok(score),
        _ => bail!(
            "line {line_number} component {component_index} has non-binary score {score}; v1 accepts only 0 or 1"
        ),
    }
}

fn safe_reason(value: Option<&Value>, pass: bool) -> Option<String> {
    // Promptfoo equals failure reasons commonly quote the raw output and
    // expected value. v1 keeps failure reasons out rather than trying to
    // redact free text after the fact.
    if !pass {
        return None;
    }
    let reason = value?.as_str()?.trim();
    if reason.is_empty()
        || reason.chars().count() > MAX_REASON_CHARS
        || reason.contains('\n')
        || reason.contains('\r')
        || reason.contains('"')
        || reason.contains('`')
        || reason.contains('{')
        || reason.contains('}')
    {
        return None;
    }
    Some(reason.to_string())
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
        .unwrap_or("promptfoo-results.jsonl")
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
    fn import_writes_verifiable_bundle_without_raw_payloads() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("results.jsonl");
        let output = dir.path().join("promptfoo.tar.gz");
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

        let code = cmd_promptfoo_jsonl(PromptfooJsonlArgs {
            input: input.clone(),
            bundle_out: output.clone(),
            source_artifact_ref: Some("results.jsonl".to_string()),
            run_id: "promptfoo_test".to_string(),
            import_time: Some("2026-04-26T12:00:00Z".to_string()),
        })
        .unwrap();
        assert_eq!(code, exit_codes::OK);

        let reader = BundleReader::open(File::open(output).unwrap()).unwrap();
        assert_eq!(reader.manifest().event_count, 2);
        let events = reader.events().collect::<Result<Vec<_>>>().unwrap();
        assert_eq!(events[0].type_, EVENT_TYPE);
        assert_eq!(events[0].source, EVENT_SOURCE);
        assert_eq!(events[0].payload["source_surface"], SOURCE_SURFACE);
        assert_eq!(events[0].payload["assertion_type"], "equals");
        assert_eq!(events[0].payload["result"]["pass"], true);
        assert_eq!(events[0].payload["result"]["score"], 1);
        assert_eq!(events[0].payload["result"]["reason"], "Assertion passed");
        assert_eq!(events[1].payload["result"]["pass"], false);
        assert_eq!(events[1].payload["result"]["score"], 0);
        assert!(events[1].payload["result"].get("reason").is_none());

        let serialized = serde_json::to_string(&events).unwrap();
        assert!(!serialized.contains("Goodbye world"));
        assert!(!serialized.contains("Hello world"));
        assert!(events[0].payload.get("componentResults").is_none());
        assert!(events[1].payload.get("componentResults").is_none());
    }

    #[test]
    fn import_fails_closed_without_component_results() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("results.jsonl");
        let output = dir.path().join("promptfoo.tar.gz");
        fs::write(&input, r#"{"gradingResult":{"pass":true,"score":1}}"#).unwrap();

        let err = cmd_promptfoo_jsonl(PromptfooJsonlArgs {
            input,
            bundle_out: output,
            source_artifact_ref: None,
            run_id: "promptfoo_test".to_string(),
            import_time: Some("2026-04-26T12:00:00Z".to_string()),
        })
        .unwrap_err();
        assert!(err
            .to_string()
            .contains("missing gradingResult.componentResults"));
    }

    #[test]
    fn import_rejects_non_binary_scores() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("results.jsonl");
        let output = dir.path().join("promptfoo.tar.gz");
        fs::write(
            &input,
            r#"{"gradingResult":{"componentResults":[{"pass":true,"score":0.5,"assertion":{"type":"equals"}}]}}"#,
        )
        .unwrap();

        let err = cmd_promptfoo_jsonl(PromptfooJsonlArgs {
            input,
            bundle_out: output,
            source_artifact_ref: None,
            run_id: "promptfoo_test".to_string(),
            import_time: Some("2026-04-26T12:00:00Z".to_string()),
        })
        .unwrap_err();
        assert!(err.to_string().contains("score must be integer 0 or 1"));
    }
}
