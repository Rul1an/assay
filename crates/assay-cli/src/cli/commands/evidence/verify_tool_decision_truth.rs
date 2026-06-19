//! EXPERIMENTAL: semantic verification of tool-decision-truth recipe rows inside an evidence bundle.
//!
//! `BundleReader::open` verifies bundle integrity (manifest hashes + Merkle root). This command layers
//! the tool-decision-truth semantics on top: it pairs every recipe-row event with the carrier event it
//! cites BY CONTENT DIGEST (`evidence_ref.digest == carrier_content_digest(carrier)`), then runs
//! `verify_recipe_row` fail-closed. A row that cites no carrier, a tampered carrier or row, a stale or
//! understated verdict, a duplicate carrier content digest, or two rows citing one digest all fail.

use crate::exit_codes;
use anyhow::{Context, Result};
use assay_core::mcp::tool_decision_truth as tdt;
use assay_core::otel::projection::TdtDecision;
use assay_evidence::bundle::BundleReader;
use assay_evidence::types::EvidenceEvent;
use clap::{Args, ValueEnum};
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::path::PathBuf;

/// Event type carrying a tool-decision-truth carrier.
const CARRIER_EVENT_TYPE: &str = "assay.tool_decision_truth.v0";
/// Schema the carrier PAYLOAD must self-declare (the event type alone is envelope metadata).
const CARRIER_SCHEMA: &str = "assay.tool_decision_truth.v0";
/// Event type carrying a recipe row that cites a carrier.
const ROW_EVENT_TYPE: &str = "assay.tool_decision_truth.recipe_row.v0";

const CLAIMS_NOT_MADE: &[&str] = &[
    "policy_correctness",
    "intent_or_maliciousness",
    "runtime_enforcement",
    "tool_result_truth",
];

#[derive(Debug, Args, Clone)]
pub struct VerifyToolDecisionTruthArgs {
    /// Evidence bundle (.tar.gz) with tool-decision-truth carrier + recipe-row events
    #[arg(value_name = "BUNDLE")]
    pub bundle: PathBuf,

    /// Output format
    #[arg(long, value_enum, default_value_t = VerifyFormat::Table)]
    pub format: VerifyFormat,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum VerifyFormat {
    Json,
    Table,
}

#[derive(Debug, Serialize)]
pub(crate) struct Report {
    pub(crate) schema: &'static str,
    pub(crate) ok: bool,
    pub(crate) carrier_count: usize,
    pub(crate) row_count: usize,
    pub(crate) verified_rows: usize,
    pub(crate) checks: Vec<Check>,
    pub(crate) claims_not_made: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
pub(crate) struct Check {
    pub(crate) id: String,
    pub(crate) ok: bool,
    pub(crate) detail: String,
}

pub fn cmd_verify_tool_decision_truth(args: VerifyToolDecisionTruthArgs) -> Result<i32> {
    let file = File::open(&args.bundle)
        .with_context(|| format!("failed to open bundle {}", args.bundle.display()))?;
    // BundleReader::open verifies bundle integrity (manifest hashes + Merkle root) before we read events.
    let reader = BundleReader::open(file).context("bundle integrity verification failed")?;
    let events = reader
        .events_vec()
        .context("failed to read bundle events")?;

    let report = build_report(&events);
    match args.format {
        VerifyFormat::Json => println!("{}", serde_json::to_string_pretty(&report)?),
        VerifyFormat::Table => print_table(&report),
    }
    Ok(if report.ok { exit_codes::OK } else { 2 })
}

fn build_report(events: &[EvidenceEvent]) -> Report {
    verify_and_collect(events).0
}

/// The single pairing + fail-closed verification pass. Returns the report AND the verified decisions
/// (the projector consumes these typed pairs, so projection never re-scans a bundle or re-pairs).
pub(crate) fn verify_and_collect(events: &[EvidenceEvent]) -> (Report, Vec<TdtDecision>) {
    let mut checks = Vec::new();

    // 1. Index carriers by content digest. Two carriers with the same content digest are ambiguous, so
    //    the pairing target is not unique and the bundle fails closed.
    let mut carriers: HashMap<String, &Value> = HashMap::new();
    let mut carrier_count = 0usize;
    for ev in events.iter().filter(|e| e.type_ == CARRIER_EVENT_TYPE) {
        carrier_count += 1;
        // The carrier PAYLOAD must self-declare the carrier schema; the event type alone is envelope
        // metadata, so a wrong-schema payload is not trusted as a carrier even if a row cites it.
        if ev.payload.get("schema").and_then(Value::as_str) != Some(CARRIER_SCHEMA) {
            checks.push(fail(
                &format!("carrier_{}_schema", ev.id),
                format!("carrier payload must declare schema {CARRIER_SCHEMA:?}"),
            ));
            continue;
        }
        match tdt::carrier_content_digest(&ev.payload) {
            Some(digest) => {
                if carriers.insert(digest.clone(), &ev.payload).is_some() {
                    checks.push(fail(
                        "duplicate_carrier_content_digest",
                        format!("more than one carrier has content digest {digest}"),
                    ));
                }
            }
            None => checks.push(fail(
                "carrier_content_digest_uncomputable",
                format!("carrier event {} could not be content-digested", ev.id),
            )),
        }
    }

    // 2. Pair each row to the carrier it cites BY CONTENT DIGEST and verify it fail-closed. Two rows
    //    citing one digest are ambiguous; a row citing no present carrier fails.
    let mut row_count = 0usize;
    let mut verified_rows = 0usize;
    let mut verified: Vec<TdtDecision> = Vec::new();
    let mut cited_digests: HashSet<String> = HashSet::new();
    for ev in events.iter().filter(|e| e.type_ == ROW_EVENT_TYPE) {
        row_count += 1;
        let row = &ev.payload;
        let Some(cited) = row
            .get("evidence_ref")
            .and_then(|r| r.get("digest"))
            .and_then(Value::as_str)
        else {
            checks.push(fail(
                &format!("row_{}_evidence_ref_digest", ev.id),
                "row has no evidence_ref.digest to pair on".to_string(),
            ));
            continue;
        };

        if !cited_digests.insert(cited.to_string()) {
            checks.push(fail(
                &format!("row_{}_duplicate_citation", ev.id),
                format!("more than one row cites carrier content digest {cited}"),
            ));
        }

        let Some(carrier) = carriers.get(cited) else {
            checks.push(fail(
                &format!("row_{}_carrier_present", ev.id),
                format!("no carrier in the bundle has content digest {cited}"),
            ));
            continue;
        };

        let run_verdict = row.get("run_verdict").and_then(Value::as_str).unwrap_or("");
        let ok = tdt::verify_recipe_row(row, carrier, run_verdict);
        if ok {
            verified_rows += 1;
            verified.push(TdtDecision {
                carrier: (**carrier).clone(),
                carrier_content_digest: cited.to_string(),
                decision_identity_digest: row
                    .get("decision_identity_digest")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                run_verdict: run_verdict.to_string(),
            });
        }
        checks.push(Check {
            id: format!("row_{}_verifies", ev.id),
            ok,
            detail: if ok {
                "row coheres with the carrier it cites".to_string()
            } else {
                "verify_recipe_row failed (content digest, identity, verdict, or binding)"
                    .to_string()
            },
        });
    }

    // 3. A tool-decision-truth verification with no rows has nothing to attest.
    if row_count == 0 {
        checks.push(fail(
            "rows_present",
            "bundle contains no tool-decision-truth recipe-row events".to_string(),
        ));
    }

    let ok = checks.iter().all(|c| c.ok);
    let report = Report {
        schema: "assay.tool_decision_truth.verify.report.v0",
        ok,
        carrier_count,
        row_count,
        verified_rows,
        checks,
        claims_not_made: CLAIMS_NOT_MADE.to_vec(),
    };
    (report, verified)
}

fn fail(id: &str, detail: String) -> Check {
    Check {
        id: id.to_string(),
        ok: false,
        detail,
    }
}

fn print_table(report: &Report) {
    println!("Tool-Decision-Truth Verification");
    println!("================================");
    println!("OK:             {}", if report.ok { "yes" } else { "no" });
    println!("Carriers:       {}", report.carrier_count);
    println!("Rows:           {}", report.row_count);
    println!("Verified rows:  {}", report.verified_rows);
    println!();
    for check in &report.checks {
        println!(
            "{:<44} {:<4} {}",
            check.id,
            if check.ok { "ok" } else { "fail" },
            check.detail
        );
    }
    println!();
    println!("Claims not made: {}", report.claims_not_made.join(", "));
}

#[cfg(test)]
mod tests {
    use super::*;
    use assay_core::mcp::policy::McpPolicy;
    use assay_core::mcp::tool_decision_truth::DecisionEvidence;
    use serde_json::json;

    const KEY: &[u8] = b"verify-tdt-test-key-v0";
    const KID: &str = "fixture-kid-v0";

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

    fn carrier(tool: &str, args: Value, order: i64, call_id: &str) -> Value {
        tdt::build_classified_record(
            &policy(),
            tool,
            &args,
            order,
            KEY,
            KID,
            "authoritative_boundary",
            call_id,
            "ok",
            "present",
            &DecisionEvidence::default(),
        )
        .unwrap()
    }

    fn row_for(carrier: &Value) -> Value {
        let verdict = carrier["decision_verdict"].as_str().unwrap();
        tdt::pack_recipe_row(carrier, verdict, "assay://evidence-event/run/0").unwrap()
    }

    fn ev(type_: &str, payload: Value, seq: u64) -> EvidenceEvent {
        EvidenceEvent::new(type_, "urn:assay:test", "run", seq, payload)
    }

    #[test]
    fn valid_carrier_and_row_verifies() {
        let c = carrier("deploy", json!({"env": "prod"}), 0, "c0");
        let r = row_for(&c);
        let report = build_report(&[ev(CARRIER_EVENT_TYPE, c, 0), ev(ROW_EVENT_TYPE, r, 1)]);
        assert!(report.ok, "expected ok, checks: {:?}", report.checks);
        assert_eq!(report.verified_rows, 1);
    }

    #[test]
    fn tampered_carrier_fails() {
        let c = carrier("deploy", json!({"env": "prod"}), 0, "c0");
        let r = row_for(&c);
        let mut tampered = c.clone();
        tampered["result_status"] = json!("error"); // changes carrier_content_digest
        let report = build_report(&[
            ev(CARRIER_EVENT_TYPE, tampered, 0),
            ev(ROW_EVENT_TYPE, r, 1),
        ]);
        assert!(!report.ok); // the row now cites a digest no carrier has
    }

    #[test]
    fn tampered_row_fails() {
        let c = carrier("deploy", json!({"env": "prod"}), 0, "c0");
        let mut r = row_for(&c);
        r["run_verdict"] = json!("mismatch"); // drifts from the bound verdict
        let report = build_report(&[ev(CARRIER_EVENT_TYPE, c, 0), ev(ROW_EVENT_TYPE, r, 1)]);
        assert!(!report.ok);
    }

    #[test]
    fn duplicate_carrier_content_digest_fails() {
        let c = carrier("deploy", json!({"env": "prod"}), 0, "c0");
        let r = row_for(&c);
        let report = build_report(&[
            ev(CARRIER_EVENT_TYPE, c.clone(), 0),
            ev(CARRIER_EVENT_TYPE, c, 1), // same content digest twice
            ev(ROW_EVENT_TYPE, r, 2),
        ]);
        assert!(!report.ok);
        assert!(report
            .checks
            .iter()
            .any(|c| c.id == "duplicate_carrier_content_digest"));
    }

    #[test]
    fn duplicate_rows_for_one_carrier_fail() {
        let c = carrier("deploy", json!({"env": "prod"}), 0, "c0");
        let r = row_for(&c);
        let report = build_report(&[
            ev(CARRIER_EVENT_TYPE, c, 0),
            ev(ROW_EVENT_TYPE, r.clone(), 1),
            ev(ROW_EVENT_TYPE, r, 2), // two rows citing one carrier digest
        ]);
        assert!(!report.ok);
        assert!(report
            .checks
            .iter()
            .any(|c| c.id.ends_with("_duplicate_citation")));
    }

    #[test]
    fn row_without_carrier_fails() {
        let c = carrier("deploy", json!({"env": "prod"}), 0, "c0");
        let r = row_for(&c);
        let report = build_report(&[ev(ROW_EVENT_TYPE, r, 0)]); // no carrier event
        assert!(!report.ok);
        assert!(report
            .checks
            .iter()
            .any(|c| c.id.ends_with("_carrier_present")));
    }

    #[test]
    fn carrier_with_wrong_payload_schema_fails() {
        // The event type says carrier, but the payload does not self-declare the carrier schema. Even a
        // row that cites it correctly by content digest must not verify.
        let mut c = carrier("deploy", json!({"env": "prod"}), 0, "c0");
        c["schema"] = json!("assay.not_a_carrier.v0");
        let verdict = c["decision_verdict"].as_str().unwrap().to_string();
        let r = tdt::pack_recipe_row(&c, &verdict, "assay://evidence-event/run/0").unwrap();
        let report = build_report(&[ev(CARRIER_EVENT_TYPE, c, 0), ev(ROW_EVENT_TYPE, r, 1)]);
        assert!(!report.ok);
        assert!(report.checks.iter().any(|c| c.id.ends_with("_schema")));
    }

    #[test]
    fn no_rows_is_not_ok() {
        let c = carrier("deploy", json!({"env": "prod"}), 0, "c0");
        let report = build_report(&[ev(CARRIER_EVENT_TYPE, c, 0)]);
        assert!(!report.ok);
        assert!(report.checks.iter().any(|c| c.id == "rows_present"));
    }
}
