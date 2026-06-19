//! EXPERIMENTAL: import a tool-decision-truth carrier into an evidence bundle as a bound recipe row.
//!
//! A carrier (`assay.tool_decision_truth.v0`) is an external artifact: the producer that mints one with
//! its HMAC key lives elsewhere. This importer ingests a single supplied carrier, validates it
//! fail-closed, binds it into a recipe row via `assay_core::mcp::tool_decision_truth::pack_recipe_row`
//! (the row cites the carrier by `carrier_content_digest`), and writes both the carrier and the row as
//! events into one bundle. The recipe row's semantic verification is the separate
//! `evidence verify-tool-decision-truth` command.

use crate::exit_codes;
use anyhow::{bail, Context, Result};
use assay_core::mcp::tool_decision_truth as tdt;
use assay_evidence::bundle::BundleWriter;
use assay_evidence::types::{EvidenceEvent, ProducerMeta};
use chrono::{DateTime, Utc};
use clap::Args;
use serde_json::Value;
use std::fs;
use std::fs::File;
use std::path::PathBuf;

/// Schema id of the carrier this importer accepts.
const CARRIER_SCHEMA: &str = "assay.tool_decision_truth.v0";
/// Event type for the carrier itself.
const CARRIER_EVENT_TYPE: &str = "assay.tool_decision_truth.v0";
/// Event type for the recipe row that cites the carrier.
const ROW_EVENT_TYPE: &str = "assay.tool_decision_truth.recipe_row.v0";
const EVENT_SOURCE: &str = "urn:assay:external:tool-decision-truth";
const DEFAULT_RUN_ID: &str = "import-tool-decision-truth";

/// Raw-argument field names that must never appear in a carrier (only digests are allowed).
const FORBIDDEN_RAW_ARG_KEYS: &[&str] = &["args", "arguments", "input", "tool_arguments"];
/// The four lattice verdicts.
const VERDICTS: &[&str] = &["match", "incomplete", "mismatch", "invalid"];
/// Append-only carrier provenance vocabulary (mirrors the builder; re-checked at this import gate).
const SOURCE_CLASSES: &[&str] = &["authoritative_boundary", "reported_trace", "inferred"];
const RESULT_STATUSES: &[&str] = &["ok", "error", "n/a"];
const IDENTITY_STATES: &[&str] = &["present", "absent", "required_missing", "invalid"];

#[derive(Debug, Args, Clone)]
pub struct ToolDecisionTruthArgs {
    /// Tool-decision-truth carrier JSON artifact (`assay.tool_decision_truth.v0`)
    #[arg(long, value_name = "PATH")]
    pub carrier: PathBuf,

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

pub fn cmd_tool_decision_truth(args: ToolDecisionTruthArgs) -> Result<i32> {
    if args.run_id.contains(':') {
        bail!("run_id cannot contain ':' because event ids use run_id:seq");
    }
    let import_time = parse_import_time(args.import_time.as_deref())?;
    let producer = ProducerMeta {
        name: "assay-cli".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        git: option_env!("ASSAY_GIT_SHA").map(str::to_string),
    };

    let carrier_bytes = fs::read_to_string(&args.carrier)
        .with_context(|| format!("failed to read carrier {}", args.carrier.display()))?;
    let carrier: Value = serde_json::from_str(&carrier_bytes)
        .with_context(|| format!("failed to parse carrier {}", args.carrier.display()))?;

    validate_carrier(&carrier)?;

    // For a single-carrier import the run is one decision, so the run verdict is the carrier's own
    // decision verdict. A carrier with no verdict cannot be bound (a recipe row needs a run verdict).
    let run_verdict = carrier
        .get("decision_verdict")
        .and_then(Value::as_str)
        .filter(|v| VERDICTS.contains(v))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "carrier decision_verdict must be one of match|incomplete|mismatch|invalid to bind a \
                 recipe row; classify the carrier first"
            )
        })?
        .to_string();

    // The row cites the carrier by carrier_content_digest; `ref` is only a deterministic bundle-local
    // pointer to the carrier event (seq 0), never the citation key.
    let reference = format!("assay://evidence-event/{}/0", args.run_id);
    let row = tdt::pack_recipe_row(&carrier, &run_verdict, &reference).ok_or_else(|| {
        anyhow::anyhow!(
            "carrier could not be bound into a recipe row (inconsistent identity digests or verdict)"
        )
    })?;

    let carrier_event =
        EvidenceEvent::new(CARRIER_EVENT_TYPE, EVENT_SOURCE, &args.run_id, 0, carrier)
            .with_time(import_time)
            .with_producer(&producer);
    let row_event = EvidenceEvent::new(ROW_EVENT_TYPE, EVENT_SOURCE, &args.run_id, 1, row)
        .with_time(import_time)
        .with_producer(&producer);

    let out_file = File::create(&args.bundle_out)
        .with_context(|| format!("failed to create bundle {}", args.bundle_out.display()))?;
    let mut writer = BundleWriter::new(out_file).with_producer(producer);
    writer.add_event(carrier_event);
    writer.add_event(row_event);
    writer
        .finish()
        .with_context(|| format!("failed to write bundle {}", args.bundle_out.display()))?;

    eprintln!(
        "Imported tool-decision-truth carrier + recipe row to {}",
        args.bundle_out.display()
    );
    Ok(exit_codes::OK)
}

/// Fail-closed carrier validation: an invalid or raw-argument-bearing carrier never reaches the bundle.
fn validate_carrier(carrier: &Value) -> Result<()> {
    let obj = carrier
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("carrier must be a JSON object"))?;

    match obj.get("schema").and_then(Value::as_str) {
        Some(CARRIER_SCHEMA) => {}
        Some(other) => bail!("carrier schema must be {CARRIER_SCHEMA:?}, got {other:?}"),
        None => bail!("carrier missing string schema"),
    }

    if let Some(key) = find_forbidden_key(carrier) {
        bail!("carrier must not contain raw argument field {key:?}; only digests are allowed");
    }

    let args_digest = obj
        .get("args_digest")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("carrier missing string args_digest"))?;
    let embedded_key_id = parse_hmac_key_id(args_digest).ok_or_else(|| {
        anyhow::anyhow!(
            "carrier args_digest must be hmac-sha256:<key_id>:<64 hex>, got {args_digest:?}"
        )
    })?;
    let key_id = obj
        .get("key_id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("carrier missing string key_id"))?;
    if key_id != embedded_key_id {
        bail!("carrier key_id {key_id:?} does not match the key_id framed in args_digest");
    }

    let oid = require_sha256(carrier, "observed_input_digest")?;
    let dpd = require_sha256(carrier, "declared_policy_digest")?;
    let di = obj
        .get("decision_identity")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow::anyhow!("carrier missing decision_identity object"))?;
    if di.get("observed_input_digest").and_then(Value::as_str) != Some(oid)
        || di.get("declared_policy_digest").and_then(Value::as_str) != Some(dpd)
    {
        bail!("carrier decision_identity must equal {{observed_input_digest, declared_policy_digest}}");
    }

    // observed_input_digest must RECOMPUTE from {tool_name, args_digest, order}; a valid shape alone
    // would let a stale or fictional digest still bind a row.
    let tool_name = obj
        .get("tool_name")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("carrier missing string tool_name"))?;
    let order = obj
        .get("order")
        .and_then(Value::as_i64)
        .ok_or_else(|| anyhow::anyhow!("carrier order must be an integer"))?;
    match tdt::observed_input_digest(tool_name, args_digest, order) {
        Some(recomputed) if recomputed.as_str() == oid => {}
        Some(_) => bail!(
            "carrier observed_input_digest does not recompute from {{tool_name, args_digest, order}}"
        ),
        None => bail!("carrier observed_input_digest could not be recomputed"),
    }

    // Provenance vocabulary is fail-closed here too: PR9a is the gate where an external carrier enters a
    // pack, so it repeats the builder's append-only vocabulary guard rather than trusting the producer.
    require_enum(carrier, "source_class", SOURCE_CLASSES)?;
    require_enum(carrier, "result_status", RESULT_STATUSES)?;
    require_enum(carrier, "identity_state", IDENTITY_STATES)?;
    carrier
        .get("call_id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("carrier missing string call_id"))?;

    match obj.get("decision_verdict") {
        None | Some(Value::Null) => {}
        Some(Value::String(v)) if VERDICTS.contains(&v.as_str()) => {}
        Some(other) => bail!(
            "carrier decision_verdict must be one of match|incomplete|mismatch|invalid or null, got {other}"
        ),
    }

    Ok(())
}

/// Recursively find the first forbidden raw-argument key anywhere in the carrier.
fn find_forbidden_key(value: &Value) -> Option<&'static str> {
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                if let Some(found) = FORBIDDEN_RAW_ARG_KEYS.iter().copied().find(|f| *f == k) {
                    return Some(found);
                }
                if let Some(found) = find_forbidden_key(v) {
                    return Some(found);
                }
            }
            None
        }
        Value::Array(arr) => arr.iter().find_map(find_forbidden_key),
        _ => None,
    }
}

/// The `key_id` framed inside an `hmac-sha256:<key_id>:<64 lowercase hex>` digest, or `None` if malformed.
fn parse_hmac_key_id(s: &str) -> Option<&str> {
    let rest = s.strip_prefix("hmac-sha256:")?;
    let (key_id, hex) = rest.split_once(':')?;
    let key_id_ok = !key_id.is_empty()
        && key_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
    let hex_ok = hex.len() == 64 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    (key_id_ok && hex_ok).then_some(key_id)
}

fn is_sha256_digest(value: &str) -> bool {
    match value.strip_prefix("sha256:") {
        Some(hex) => hex.len() == 64 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')),
        None => false,
    }
}

fn require_sha256<'a>(obj: &'a Value, field: &str) -> Result<&'a str> {
    let value = obj
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("carrier missing string {field}"))?;
    if !is_sha256_digest(value) {
        bail!("carrier {field} must be sha256:<64 hex>, got {value:?}");
    }
    Ok(value)
}

fn require_enum(carrier: &Value, field: &str, allowed: &[&str]) -> Result<()> {
    match carrier.get(field).and_then(Value::as_str) {
        Some(value) if allowed.contains(&value) => Ok(()),
        Some(value) => bail!(
            "carrier {field} {value:?} is not one of {}",
            allowed.join("|")
        ),
        None => bail!("carrier missing string {field}"),
    }
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
    use assay_core::mcp::policy::McpPolicy;
    use assay_core::mcp::tool_decision_truth::DecisionEvidence;
    use assay_evidence::bundle::BundleReader;
    use serde_json::json;

    fn policy() -> McpPolicy {
        serde_json::from_value(json!({
            "version": "1",
            "tools": {"allow": ["deploy"], "deny": ["delete_all"]},
            "schemas": {"deploy": {"type": "object", "required": ["env"],
                "properties": {"env": {"enum": ["staging", "prod"]}}}},
            "enforcement": {"unconstrained_tools": "warn"}
        }))
        .unwrap()
    }

    fn sample_carrier() -> Value {
        tdt::build_classified_record(
            &policy(),
            "deploy",
            &json!({"env": "prod"}),
            0,
            b"pack-import-test-key-v0",
            "fixture-kid-v0",
            "authoritative_boundary",
            "c0",
            "ok",
            "present",
            &DecisionEvidence::default(),
        )
        .unwrap()
    }

    fn run_import(
        carrier: &Value,
        run_id: &str,
    ) -> Result<(std::path::PathBuf, tempfile::TempDir)> {
        let dir = tempfile::tempdir().unwrap();
        let carrier_path = dir.path().join("carrier.json");
        let out = dir.path().join("tdt.tar.gz");
        fs::write(
            &carrier_path,
            serde_json::to_string_pretty(carrier).unwrap(),
        )
        .unwrap();
        cmd_tool_decision_truth(ToolDecisionTruthArgs {
            carrier: carrier_path,
            bundle_out: out.clone(),
            run_id: run_id.to_string(),
            import_time: Some("2026-06-19T00:00:00Z".to_string()),
        })
        .map(|_| (out, dir))
    }

    #[test]
    fn import_writes_carrier_and_row_bundle_that_verifies() {
        let carrier = sample_carrier();
        let (out, _dir) = run_import(&carrier, "tdt_test").unwrap();

        let reader = BundleReader::open(File::open(&out).unwrap()).unwrap();
        assert_eq!(reader.manifest().event_count, 2);
        let events = reader.events_vec().unwrap();
        assert_eq!(events[0].type_, CARRIER_EVENT_TYPE);
        assert_eq!(events[1].type_, ROW_EVENT_TYPE);

        // The emitted row verifies fail-closed against the emitted carrier.
        let bundled_carrier = &events[0].payload;
        let row = &events[1].payload;
        assert!(tdt::verify_recipe_row(row, bundled_carrier, "match"));
        // The row cites the carrier by content digest, not by call_id.
        assert_eq!(
            row["evidence_ref"]["digest"],
            json!(tdt::carrier_content_digest(bundled_carrier).unwrap())
        );

        // No raw arguments leaked anywhere.
        let serialized = serde_json::to_string(&events).unwrap();
        assert!(!serialized.contains("\"arguments\""));
        assert!(!serialized.contains("\"tool_arguments\""));
    }

    #[test]
    fn import_rejects_raw_arguments() {
        let mut carrier = sample_carrier();
        carrier["arguments"] = json!({"env": "prod"});
        let err = run_import(&carrier, "tdt_test").unwrap_err();
        assert!(err.to_string().contains("raw argument field"));
    }

    #[test]
    fn import_rejects_malformed_args_digest() {
        let mut carrier = sample_carrier();
        carrier["args_digest"] = json!("not-an-hmac");
        let err = run_import(&carrier, "tdt_test").unwrap_err();
        assert!(err.to_string().contains("args_digest must be hmac-sha256"));
    }

    #[test]
    fn import_rejects_non_vocab_verdict() {
        let mut carrier = sample_carrier();
        carrier["decision_verdict"] = json!("approved");
        let err = run_import(&carrier, "tdt_test").unwrap_err();
        assert!(err.to_string().contains("decision_verdict must be one of"));
    }

    #[test]
    fn import_rejects_null_verdict_cannot_bind() {
        let mut carrier = sample_carrier();
        carrier["decision_verdict"] = Value::Null;
        let err = run_import(&carrier, "tdt_test").unwrap_err();
        assert!(err.to_string().contains("to bind a"));
    }

    #[test]
    fn import_rejects_run_id_with_colon() {
        let carrier = sample_carrier();
        let err = run_import(&carrier, "bad:run").unwrap_err();
        assert!(err.to_string().contains("run_id cannot contain ':'"));
    }

    #[test]
    fn import_rejects_stale_observed_input_digest() {
        // Changing tool_name without updating observed_input_digest fails (the digest is recomputed).
        let mut carrier = sample_carrier();
        carrier["tool_name"] = json!("delete_all");
        let err = run_import(&carrier, "tdt_test").unwrap_err();
        assert!(err
            .to_string()
            .contains("observed_input_digest does not recompute"));

        // Same for a changed order.
        let mut carrier2 = sample_carrier();
        carrier2["order"] = json!(7);
        let err2 = run_import(&carrier2, "tdt_test").unwrap_err();
        assert!(err2
            .to_string()
            .contains("observed_input_digest does not recompute"));
    }

    #[test]
    fn import_rejects_out_of_vocab_provenance() {
        for (field, bad) in [
            ("source_class", json!("made_up")),
            ("result_status", json!("maybe")),
            ("identity_state", json!("unknown")),
        ] {
            let mut carrier = sample_carrier();
            carrier[field] = bad;
            let err = run_import(&carrier, "tdt_test").unwrap_err();
            assert!(
                err.to_string().contains(&format!("carrier {field}")),
                "field {field} must be rejected: {err}"
            );
        }
        // Missing call_id is rejected.
        let mut carrier = sample_carrier();
        carrier.as_object_mut().unwrap().remove("call_id");
        let err = run_import(&carrier, "tdt_test").unwrap_err();
        assert!(err.to_string().contains("call_id"));
    }
}
