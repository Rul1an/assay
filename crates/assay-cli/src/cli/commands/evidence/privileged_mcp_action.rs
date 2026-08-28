//! EXPERIMENTAL: import privileged-mcp-action producer NDJSON records into one evidence bundle.
//!
//! Inputs are the shipped enforcing-proxy carriers: `assay.enforcement_decision.v0` records
//! (required file), plus optional denied-observation and `assay.manifest_establish.v0` files.
//! Every record is wrapped byte-faithful as an evidence event whose type is the record's own
//! `schema` member. The importer is profile-agnostic: it does not select a profile version, does
//! not autodetect one, and does not stamp a profile id onto the bundle. Cardinality, vocabularies,
//! and binding belong to `evidence verify-privileged-mcp-action`. The conformance corpus requires
//! that semantically invalid bundles (for example two decisions) stay producible.

use crate::exit_codes;
use anyhow::{bail, Context, Result};
use assay_core::mcp::ingest::{parse_mcp_transcript_bounded, McpTranscriptLimits};
use assay_core::mcp::McpInputFormat;
use assay_evidence::bundle::BundleWriter;
use assay_evidence::types::{EvidenceEvent, ProducerMeta};
use chrono::{DateTime, Utc};
use clap::{Args, ValueEnum};
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

    /// Optional NDJSON file of denied-call observation records (byte-faithful)
    #[arg(long, value_name = "PATH")]
    pub denied_observations: Option<PathBuf>,

    /// Optional NDJSON file of assay.manifest_establish.v0 records
    #[arg(long, value_name = "PATH")]
    pub manifest_establish: Option<PathBuf>,

    /// Optional MCP transcript to inspect beside the producer-authored profile carriers
    #[arg(long, value_name = "PATH", requires = "mcp_format")]
    pub mcp_transcript: Option<PathBuf>,

    /// Input format of --mcp-transcript
    #[arg(long, value_enum, requires = "mcp_transcript")]
    pub mcp_format: Option<PrivilegedMcpTranscriptFormat>,

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

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum PrivilegedMcpTranscriptFormat {
    Inspector,
    Jsonrpc,
    StreamableHttp,
    HttpSse,
}

pub fn cmd_privileged_mcp_action(args: PrivilegedMcpActionArgs) -> Result<i32> {
    if args.run_id.contains(':') {
        bail!("run_id cannot contain ':' because event ids use run_id:seq");
    }
    inspect_optional_mcp_transcript(&args)?;
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

fn inspect_optional_mcp_transcript(args: &PrivilegedMcpActionArgs) -> Result<()> {
    let (path, format) = match (&args.mcp_transcript, args.mcp_format) {
        (None, None) => return Ok(()),
        (Some(_), None) => bail!("--mcp-transcript requires --mcp-format"),
        (None, Some(_)) => bail!("--mcp-format requires --mcp-transcript"),
        (Some(path), Some(format)) => (path, format),
    };
    let file = File::open(path)
        .with_context(|| format!("failed to open MCP transcript {}", path.display()))?;
    let format = match format {
        PrivilegedMcpTranscriptFormat::Inspector => McpInputFormat::Inspector,
        PrivilegedMcpTranscriptFormat::Jsonrpc => McpInputFormat::JsonRpc,
        PrivilegedMcpTranscriptFormat::StreamableHttp => McpInputFormat::StreamableHttp,
        PrivilegedMcpTranscriptFormat::HttpSse => McpInputFormat::HttpSse,
    };
    parse_mcp_transcript_bounded(file, format, McpTranscriptLimits::default())
        .map_err(|error| anyhow::anyhow!("MCP transcript ingest failed: {error}"))?;
    Ok(())
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

    const NOTIFICATION: &str = r#"{"jsonrpc":"2.0","method":"notifications/progress","params":{}}"#;

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
            mcp_transcript: None,
            mcp_format: None,
            bundle_out: dir.join("out.bundle.tar.gz"),
            run_id: DEFAULT_RUN_ID.to_string(),
            import_time: Some("2026-07-24T00:00:00Z".to_string()),
        }
    }

    #[test]
    fn producer_lane_import_wraps_records_byte_faithful_in_declared_order() {
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
    fn producer_lane_import_does_not_enforce_profile_cardinality() {
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
    fn producer_lane_import_rejects_record_without_schema() {
        let dir = tempfile::tempdir().unwrap();
        let decisions = write_ndjson(dir.path(), "dec.ndjson", &[json!({"decision": "deny"})]);
        let err = cmd_privileged_mcp_action(args(dir.path(), decisions)).unwrap_err();
        assert!(err.to_string().contains("no string schema"));
    }

    #[test]
    fn producer_lane_import_rejects_run_id_with_colon() {
        let dir = tempfile::tempdir().unwrap();
        let decisions = write_ndjson(dir.path(), "dec.ndjson", &[decision("deny")]);
        let mut a = args(dir.path(), decisions);
        a.run_id = "bad:run".to_string();
        let err = cmd_privileged_mcp_action(a).unwrap_err();
        assert!(err.to_string().contains("run_id cannot contain ':'"));
    }

    #[test]
    fn producer_lane_import_rejects_empty_inputs() {
        let dir = tempfile::tempdir().unwrap();
        let decisions = write_ndjson(dir.path(), "dec.ndjson", &[]);
        let err = cmd_privileged_mcp_action(args(dir.path(), decisions)).unwrap_err();
        assert!(err.to_string().contains("nothing to import"));
    }

    fn profile_allow_decision() -> Value {
        json!({
            "schema": "assay.enforcement_decision.v0",
            "caller": {"id": "ci-agent"},
            "tool": {
                "name": "github.add_deploy_key",
                "action_class": "github_deploy_key"
            },
            "action": {
                "verb": "create",
                "resource_type": "github_deploy_key",
                "target": {
                    "provider": "github",
                    "owner": "acme",
                    "repo": "prod-app"
                },
                "target_digest": "sha256:c3ff823d7fb2ee33b9f1a3f7be6eaf849acb980b6ec960731506436b56384dfc"
            },
            "decision": "allow",
            "reason": "allow",
            "fail_closed": false,
            "drift_state": "satisfied",
            "credential_alias": "gh-deploy",
            "non_claims": [
                "policy decision only; does not assert or verify the upstream side effect (stays asserted, E9 ladder)",
                "an allow is the decision to forward; it does not assert the call reached or was performed by the upstream (a transport failure surfaces as proxy_failed, not here)",
                "credential referenced by alias only, never the token or declared scopes",
                "deny is fail-closed caution and allow is a policy decision — neither is a maliciousness verdict",
                "not the observation artifact (assay.mcp_manifest_observed.v0) and not the mechanism artifact (assay.enforcement_health.v0)"
            ]
        })
    }

    fn expected_allow_report() -> Value {
        json!({
            "schema": "assay.privileged_mcp_action.verify.report.v0",
            "profile": "privileged-mcp-action/v0",
            "profile_selection": "default",
            "input_profile": null,
            "input_profile_status": "undeclared_legacy",
            "bundle_integrity": "pass",
            "verdict": "valid",
            "claims": {
                "policy_decision_recorded": {
                    "status": "confirmed",
                    "source_class": "producer_reported"
                },
                "caller_visible_denial": {"status": "incomplete"},
                "upstream_delivery": {"status": "incomplete"},
                "external_side_effect": {"status": "incomplete"}
            },
            "findings": [],
            "non_claims": [
                "allow does not prove upstream delivery",
                "deny does not establish maliciousness",
                "caller-visible denial does not prove external side-effect absence",
                "bundle integrity does not upgrade source class"
            ]
        })
    }

    fn write_transcript(dir: &Path, name: &str, text: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn e2e_lane_wire_era_forms_round_trip_to_the_same_frozen_profile_report() {
        use super::super::verify_privileged_mcp_action::verify_bundle_report;

        let vectors = [
            (
                "legacy-missing",
                r#"{"transport":"streamable-http","transport_context":{"headers":{"MCP-Protocol-Version":"2025-06-18"}},"entries":[{"request":{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"github.add_deploy_key","arguments":{}}}},{"response":{"jsonrpc":"2.0","id":1,"result":{"content":[]}}}]}"#,
            ),
            (
                "modern-complete",
                r#"{"transport":"streamable-http","transport_context":{"headers":{"MCP-Protocol-Version":"2026-07-28"}},"entries":[{"request":{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"github.add_deploy_key","arguments":{},"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}}}}},{"response":{"jsonrpc":"2.0","id":1,"result":{"content":[],"resultType":"complete"}}}]}"#,
            ),
            (
                "modern-input-required",
                r#"{"transport":"streamable-http","transport_context":{"headers":{"MCP-Protocol-Version":"2026-07-28"}},"entries":[{"request":{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"github.add_deploy_key","arguments":{},"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}}}}},{"response":{"jsonrpc":"2.0","id":1,"result":{"resultType":"input_required","inputRequests":{"elicitation":{"method":"elicitation/create","params":{"message":"m","requestedSchema":{"type":"object","properties":{}}}}}}}}]}"#,
            ),
        ];

        for (name, transcript) in vectors {
            let dir = tempfile::tempdir().unwrap();
            let decisions = write_ndjson(dir.path(), "dec.ndjson", &[profile_allow_decision()]);
            let mut import = args(dir.path(), decisions);
            import.mcp_transcript =
                Some(write_transcript(dir.path(), "transcript.json", transcript));
            import.mcp_format = Some(PrivilegedMcpTranscriptFormat::StreamableHttp);
            cmd_privileged_mcp_action(import.clone())
                .unwrap_or_else(|error| panic!("{name}: {error:#}"));
            let report = verify_bundle_report(&import.bundle_out);
            assert_eq!(
                serde_json::to_value(report).unwrap(),
                expected_allow_report(),
                "{name}"
            );
        }
    }

    #[test]
    fn every_cli_transcript_format_reaches_its_parser_and_preserves_the_profile() {
        use super::super::verify_privileged_mcp_action::verify_bundle_report;

        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "github.add_deploy_key",
                "arguments": {},
                "_meta": {
                    "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                    "io.modelcontextprotocol/clientCapabilities": {}
                }
            }
        });
        let response = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {"content": [], "resultType": "complete"}
        });
        let header = json!({"MCP-Protocol-Version": "2026-07-28"});
        let vectors = [
            (
                "inspector",
                json!({"events": [request.clone(), response.clone()]}).to_string(),
                PrivilegedMcpTranscriptFormat::Inspector,
            ),
            (
                "jsonrpc",
                format!("{request}\n{response}\n"),
                PrivilegedMcpTranscriptFormat::Jsonrpc,
            ),
            (
                "streamable-http",
                json!({
                    "transport": "streamable-http",
                    "transport_context": {"headers": header.clone()},
                    "entries": [
                        {"request": request.clone()},
                        {"response": response.clone()}
                    ]
                })
                .to_string(),
                PrivilegedMcpTranscriptFormat::StreamableHttp,
            ),
            (
                "http-sse",
                json!({
                    "transport": "http-sse",
                    "transport_context": {"headers": header},
                    "entries": [
                        {"sse": {"event": "message", "data": request}},
                        {"sse": {"event": "message", "data": response}}
                    ]
                })
                .to_string(),
                PrivilegedMcpTranscriptFormat::HttpSse,
            ),
        ];

        for (name, transcript, format) in vectors {
            let dir = tempfile::tempdir().unwrap();
            let decisions = write_ndjson(dir.path(), "dec.ndjson", &[profile_allow_decision()]);
            let mut import = args(dir.path(), decisions);
            import.mcp_transcript = Some(write_transcript(
                dir.path(),
                &format!("{name}.json"),
                &transcript,
            ));
            import.mcp_format = Some(format);
            cmd_privileged_mcp_action(import.clone())
                .unwrap_or_else(|error| panic!("{name}: {error:#}"));
            let report = verify_bundle_report(&import.bundle_out);
            assert_eq!(
                serde_json::to_value(report).unwrap(),
                expected_allow_report(),
                "{name}"
            );
        }
    }

    #[test]
    fn producer_lane_malformed_wire_input_is_refused_before_a_bundle_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let decisions = write_ndjson(dir.path(), "dec.ndjson", &[profile_allow_decision()]);
        let mut import = args(dir.path(), decisions);
        import.mcp_transcript = Some(write_transcript(
            dir.path(),
            "transcript.json",
            r#"{"ATTACKER_SENTINEL":"#,
        ));
        import.mcp_format = Some(PrivilegedMcpTranscriptFormat::StreamableHttp);
        let error = cmd_privileged_mcp_action(import.clone()).unwrap_err();
        assert!(
            error
                .to_string()
                .starts_with("MCP transcript ingest failed:"),
            "{error:#}"
        );
        assert!(error.to_string().contains("MCP transcript is invalid"));
        assert!(!format!("{error:?}").contains("ATTACKER_SENTINEL"));
        assert!(!import.bundle_out.exists());
    }

    #[test]
    fn producer_lane_deny_import_accepts_an_observed_jsonrpc_error_without_deriving_from_it() {
        let dir = tempfile::tempdir().unwrap();
        let decisions = write_ndjson(dir.path(), "dec.ndjson", &[decision("deny")]);
        let transcript = r#"{"transport":"streamable-http","transport_context":{"headers":{"MCP-Protocol-Version":"2026-07-28"}},"entries":[{"request":{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"github.add_deploy_key","arguments":{},"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}}}}},{"response":{"jsonrpc":"2.0","id":1,"error":{"code":-32042,"message":"ATTACKER_SENTINEL"}}}]}"#;
        let mut import = args(dir.path(), decisions);
        import.mcp_transcript = Some(write_transcript(
            dir.path(),
            "deny-transcript.json",
            transcript,
        ));
        import.mcp_format = Some(PrivilegedMcpTranscriptFormat::StreamableHttp);

        assert_eq!(
            cmd_privileged_mcp_action(import.clone()).unwrap(),
            exit_codes::OK
        );
        let reader = BundleReader::open(File::open(&import.bundle_out).unwrap()).unwrap();
        let events = reader.events_vec().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].payload, decision("deny"));
        assert!(
            !serde_json::to_string(&events)
                .unwrap()
                .contains("ATTACKER_SENTINEL"),
            "the optional wire observation must not become producer evidence"
        );
    }

    #[test]
    fn producer_lane_transcript_and_format_must_be_supplied_together() {
        let dir = tempfile::tempdir().unwrap();
        let decisions = write_ndjson(dir.path(), "dec.ndjson", &[profile_allow_decision()]);
        let mut transcript_only = args(dir.path(), decisions.clone());
        transcript_only.mcp_transcript = Some(write_transcript(
            dir.path(),
            "transcript.json",
            NOTIFICATION,
        ));
        assert!(cmd_privileged_mcp_action(transcript_only)
            .unwrap_err()
            .to_string()
            .contains("--mcp-format"));

        let mut format_only = args(dir.path(), decisions);
        format_only.mcp_format = Some(PrivilegedMcpTranscriptFormat::Jsonrpc);
        assert!(cmd_privileged_mcp_action(format_only)
            .unwrap_err()
            .to_string()
            .contains("--mcp-transcript"));
    }
}
