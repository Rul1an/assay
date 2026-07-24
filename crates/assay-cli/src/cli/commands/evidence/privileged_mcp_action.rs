//! EXPERIMENTAL: import privileged-mcp-action producer NDJSON records into one evidence bundle.
//!
//! Inputs are the shipped enforcing-proxy carriers of the `privileged-mcp-action/v0` open profile:
//! `assay.enforcement_decision.v0` records (required file), plus optional
//! `assay.denied_call_observation.v0` and `assay.manifest_establish.v0` files. Every record is
//! wrapped byte-faithful as an evidence event whose type is the record's own `schema` member. The
//! importer deliberately enforces NO profile semantics (cardinality, vocabularies, binding): those
//! belong to the separate `evidence verify-privileged-mcp-action` verifier, and the conformance
//! corpus requires that semantically invalid bundles (for example two decisions) stay producible.

use crate::exit_codes;
use anyhow::{bail, Context, Result};
use assay_evidence::bundle::BundleWriter;
use assay_evidence::types::{EvidenceEvent, ProducerMeta};
use chrono::{DateTime, Utc};
use clap::Args;
use serde_json::Value;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

const EVENT_SOURCE: &str = "urn:assay:external:privileged-mcp-action";
const DEFAULT_RUN_ID: &str = "import-privileged-mcp-action";

#[derive(Debug, Args, Clone)]
pub struct PrivilegedMcpActionArgs {
    /// NDJSON file of assay.enforcement_decision.v0 records
    #[arg(long, value_name = "PATH")]
    pub decisions: PathBuf,

    /// Optional NDJSON file of assay.denied_call_observation.v0 records
    #[arg(long, value_name = "PATH")]
    pub denied_observations: Option<PathBuf>,

    /// Optional NDJSON file of assay.manifest_establish.v0 records
    #[arg(long, value_name = "PATH")]
    pub manifest_establish: Option<PathBuf>,

    /// Output Assay evidence bundle path (.tar.gz)
    #[arg(long, alias = "out", value_name = "PATH")]
    pub bundle_out: PathBuf,

    /// Assay import run id used for provenance and event ids
    #[arg(long, default_value = DEFAULT_RUN_ID)]
    pub run_id: String,

    /// Import timestamp for deterministic fixtures (RFC3339 UTC recommended)
    #[arg(long)]
    pub import_time: Option<String>,
}

pub fn cmd_privileged_mcp_action(args: PrivilegedMcpActionArgs) -> Result<i32> {
    if args.run_id.contains(':') {
        bail!("run_id cannot contain ':' because event ids use run_id:seq");
    }
    let import_time = parse_import_time(args.import_time.as_deref())?;
    let producer = ProducerMeta {
        name: "assay-cli".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        git: option_env!("ASSAY_GIT_SHA").map(str::to_string),
    };

    // Sequence order is fixed: all decisions in file order, then observations, then establish
    // records. The order is provenance only; the verifier selects by payload schema, never by seq.
    let mut records = read_ndjson_records(&args.decisions)?;
    if let Some(path) = &args.denied_observations {
        records.extend(read_ndjson_records(path)?);
    }
    if let Some(path) = &args.manifest_establish {
        records.extend(read_ndjson_records(path)?);
    }
    if records.is_empty() {
        bail!(
            "no records found in {} (or the optional inputs); nothing to import",
            args.decisions.display()
        );
    }

    let out_file = File::create(&args.bundle_out)
        .with_context(|| format!("failed to create bundle {}", args.bundle_out.display()))?;
    let mut writer = BundleWriter::new(out_file).with_producer(producer.clone());
    for (seq, (schema, payload)) in records.into_iter().enumerate() {
        let event = EvidenceEvent::new(&schema, EVENT_SOURCE, &args.run_id, seq as u64, payload)
            .with_time(import_time)
            .with_producer(&producer);
        writer.add_event(event);
    }
    writer
        .finish()
        .with_context(|| format!("failed to write bundle {}", args.bundle_out.display()))?;

    eprintln!(
        "Imported privileged-mcp-action records to {}",
        args.bundle_out.display()
    );
    Ok(exit_codes::OK)
}

/// Read one NDJSON file into `(schema, payload)` pairs, byte-faithful.
///
/// Each non-blank line must parse as a JSON object carrying a string `schema` member: that string
/// becomes the event type. Nothing else about the record is inspected here.
fn read_ndjson_records(path: &Path) -> Result<Vec<(String, Value)>> {
    let file =
        File::open(path).with_context(|| format!("failed to open records {}", path.display()))?;
    let mut records = Vec::new();
    for (lineno, line) in BufReader::new(file).lines().enumerate() {
        let line = line.with_context(|| format!("failed to read {}", path.display()))?;
        if line.trim().is_empty() {
            continue;
        }
        let payload: Value = serde_json::from_str(&line)
            .with_context(|| format!("{} line {}: invalid JSON", path.display(), lineno + 1))?;
        let schema = payload
            .get("schema")
            .and_then(Value::as_str)
            .map(str::to_string);
        match schema {
            Some(schema) => records.push((schema, payload)),
            None => bail!(
                "{} line {}: record has no string schema member; cannot type the event",
                path.display(),
                lineno + 1
            ),
        }
    }
    Ok(records)
}

fn parse_import_time(value: Option<&str>) -> Result<DateTime<Utc>> {
    match value {
        Some(value) => Ok(DateTime::parse_from_rfc3339(value)
            .with_context(|| format!("invalid --import-time {value:?}; expected RFC3339"))?
            .with_timezone(&Utc)),
        None => Ok(Utc::now()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assay_evidence::bundle::BundleReader;
    use serde_json::json;

    fn decision(decision: &str) -> Value {
        json!({
            "schema": "assay.enforcement_decision.v0",
            "decision": decision,
            "tool": {"name": "github.add_deploy_key"}
        })
    }

    fn observation() -> Value {
        json!({
            "schema": "assay.denied_call_observation.v0",
            "call": {"tool_name": "github.add_deploy_key"}
        })
    }

    fn write_ndjson(dir: &Path, name: &str, records: &[Value]) -> PathBuf {
        let path = dir.join(name);
        let mut body = String::new();
        for r in records {
            body.push_str(&serde_json::to_string(r).unwrap());
            body.push('\n');
        }
        std::fs::write(&path, body).unwrap();
        path
    }

    fn args(dir: &Path, decisions: PathBuf) -> PrivilegedMcpActionArgs {
        PrivilegedMcpActionArgs {
            decisions,
            denied_observations: None,
            manifest_establish: None,
            bundle_out: dir.join("out.bundle.tar.gz"),
            run_id: DEFAULT_RUN_ID.to_string(),
            import_time: Some("2026-07-24T00:00:00Z".to_string()),
        }
    }

    #[test]
    fn import_wraps_records_byte_faithful_in_declared_order() {
        let dir = tempfile::tempdir().unwrap();
        let decisions = write_ndjson(dir.path(), "dec.ndjson", &[decision("deny")]);
        let observations = write_ndjson(dir.path(), "obs.ndjson", &[observation()]);
        let mut a = args(dir.path(), decisions);
        a.denied_observations = Some(observations);
        let code = cmd_privileged_mcp_action(a.clone()).unwrap();
        assert_eq!(code, exit_codes::OK);

        let reader = BundleReader::open(File::open(&a.bundle_out).unwrap()).unwrap();
        assert_eq!(reader.manifest().event_count, 2);
        let events = reader.events_vec().unwrap();
        // Event type equals the record's own schema member; payload is the record byte-faithful.
        assert_eq!(events[0].type_, "assay.enforcement_decision.v0");
        assert_eq!(events[0].payload, decision("deny"));
        assert_eq!(events[1].type_, "assay.denied_call_observation.v0");
        assert_eq!(events[1].payload, observation());
        assert_eq!(events[0].source, EVENT_SOURCE);
    }

    #[test]
    fn import_does_not_enforce_profile_cardinality() {
        // Two decisions in one file must stay producible (the verifier, not the importer, rejects
        // them; the conformance corpus's two-decision vector depends on this split).
        let dir = tempfile::tempdir().unwrap();
        let decisions = write_ndjson(
            dir.path(),
            "dec.ndjson",
            &[decision("deny"), decision("allow")],
        );
        let a = args(dir.path(), decisions);
        cmd_privileged_mcp_action(a.clone()).unwrap();
        let reader = BundleReader::open(File::open(&a.bundle_out).unwrap()).unwrap();
        assert_eq!(reader.manifest().event_count, 2);
    }

    #[test]
    fn import_rejects_record_without_schema() {
        let dir = tempfile::tempdir().unwrap();
        let decisions = write_ndjson(dir.path(), "dec.ndjson", &[json!({"decision": "deny"})]);
        let err = cmd_privileged_mcp_action(args(dir.path(), decisions)).unwrap_err();
        assert!(err.to_string().contains("no string schema"));
    }

    #[test]
    fn import_rejects_run_id_with_colon() {
        let dir = tempfile::tempdir().unwrap();
        let decisions = write_ndjson(dir.path(), "dec.ndjson", &[decision("deny")]);
        let mut a = args(dir.path(), decisions);
        a.run_id = "bad:run".to_string();
        let err = cmd_privileged_mcp_action(a).unwrap_err();
        assert!(err.to_string().contains("run_id cannot contain ':'"));
    }

    #[test]
    fn import_rejects_empty_inputs() {
        let dir = tempfile::tempdir().unwrap();
        let decisions = write_ndjson(dir.path(), "dec.ndjson", &[]);
        let err = cmd_privileged_mcp_action(args(dir.path(), decisions)).unwrap_err();
        assert!(err.to_string().contains("nothing to import"));
    }
}
