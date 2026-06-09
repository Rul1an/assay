//! `assay evidence verify-mcp-supersession` — independent-consumer evaluation of decision-record
//! supersession for SEP-2828-style execution records.
//!
//! Given several decision records that share a call binding (`backLink`), it reports which decision
//! is effective. The contract, matching the upstream SEP-2828 discussion:
//!
//! - the latest `decidedAt` wins;
//! - if two records share the same `decidedAt` and there is no explicit ordering field
//!   (`decisionDerived.sequence`), the supersession is **ambiguous / non-conformant** — the verifier
//!   does not guess from file order, arrival order, or the record nonce, because a nonce is unique
//!   per record, not an ordering field, and an arbitrary-but-deterministic winner can mask a producer
//!   that emitted two records that should never have tied.
//!
//! This is the consumer side only: it reads committed records and does not verify signatures, issuer
//! trust, freshness, or runtime truth.

use anyhow::{Context, Result};
use clap::{Args, ValueEnum};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Args, Clone)]
pub struct McpSupersessionArgs {
    /// JSON array of server-side decision records that may share a call binding.
    #[arg(long)]
    pub decisions: PathBuf,

    /// Output format
    #[arg(long, value_enum, default_value_t = SupersessionFormat::Table)]
    pub format: SupersessionFormat,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum SupersessionFormat {
    Json,
    Table,
}

#[derive(Debug, Serialize)]
struct SupersessionReport {
    schema: &'static str,
    ok: bool,
    verification_scope: VerificationScope,
    groups: Vec<GroupReport>,
    claims_not_made: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
struct VerificationScope {
    role: &'static str,
    note: &'static str,
}

#[derive(Debug, Serialize)]
struct GroupReport {
    backlink_key: String,
    count: usize,
    verdict: &'static str,
    /// Stable, machine-readable reason; CI consumers key on this, not on `detail` prose.
    reason_code: &'static str,
    effective_decided_at: Option<String>,
    effective_decision: Option<String>,
    detail: String,
}

pub fn cmd_verify_mcp_supersession(args: McpSupersessionArgs) -> Result<i32> {
    let body = fs::read_to_string(&args.decisions)
        .with_context(|| format!("failed to read {}", args.decisions.display()))?;
    let parsed: Value = serde_json::from_str(&body)
        .with_context(|| format!("failed to parse {}", args.decisions.display()))?;
    let records = parsed
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("--decisions must be a JSON array of decision records"))?;

    // Group by backLink pair key. Records with no backLink are reported as their own ungrouped entry.
    let mut groups: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
    for record in records {
        groups.entry(backlink_key(record)).or_default().push(record);
    }

    let group_reports: Vec<GroupReport> = groups
        .into_iter()
        .map(|(key, records)| evaluate_group(key, &records))
        .collect();

    let ok = group_reports.iter().all(|g| g.verdict == "resolved");
    let report = SupersessionReport {
        schema: "assay.mcp.execution-record-supersession.report.v0",
        ok,
        verification_scope: VerificationScope {
            role: "independent-consumer",
            note: "Assay evaluates supersession ordering from committed decision records only; it does not verify signatures, issuer trust, freshness, or runtime truth.",
        },
        groups: group_reports,
        claims_not_made: vec![
            "signature_verification",
            "issuer_key_trust",
            "decided_at_clock_truth",
            // `sequence` is read from the canonical decisionDerived content; Assay verifies no
            // signatures, so a sequence-resolved ordering is asserted-content, not independently
            // verified ordering.
            "sequence_ordering_is_asserted_content_not_verified",
            "policy_correctness",
            "runtime_side_effect_truth",
        ],
    };

    match args.format {
        SupersessionFormat::Json => println!("{}", serde_json::to_string_pretty(&report)?),
        SupersessionFormat::Table => print_table(&report),
    }
    Ok(if report.ok { 0 } else { 2 })
}

fn evaluate_group(key: String, records: &[&Value]) -> GroupReport {
    let count = records.len();
    if count == 1 {
        return GroupReport {
            backlink_key: key,
            count,
            verdict: "resolved",
            reason_code: "supersession_resolved_single",
            effective_decided_at: decided_at(records[0]),
            effective_decision: decision_value(records[0]),
            detail: "single decision for this call binding".to_string(),
        };
    }

    // Latest decidedAt wins. RFC 3339 UTC timestamps compare lexicographically; a missing decidedAt
    // cannot be ordered, so a group with any missing decidedAt is ambiguous.
    if records.iter().any(|r| decided_at(r).is_none()) {
        return ambiguous(
            key,
            count,
            "supersession_ambiguous_missing_decided_at",
            "a decision record is missing decidedAt; cannot order",
        );
    }
    let max_time = records.iter().filter_map(|r| decided_at(r)).max().unwrap();
    let leaders: Vec<&Value> = records
        .iter()
        .copied()
        .filter(|r| decided_at(r).as_deref() == Some(max_time.as_str()))
        .collect();

    if leaders.len() == 1 {
        return GroupReport {
            backlink_key: key,
            count,
            verdict: "resolved",
            reason_code: "supersession_resolved_latest_decided_at",
            effective_decided_at: Some(max_time),
            effective_decision: decision_value(leaders[0]),
            detail: "latest decidedAt is unique".to_string(),
        };
    }

    // Tie on decidedAt: only an explicit ordering field resolves it; the nonce does not.
    if leaders.iter().all(|r| sequence(r).is_some()) {
        let max_seq = leaders.iter().filter_map(|r| sequence(r)).max().unwrap();
        let seq_leaders: Vec<&Value> = leaders
            .iter()
            .copied()
            .filter(|r| sequence(r) == Some(max_seq))
            .collect();
        if seq_leaders.len() == 1 {
            return GroupReport {
                backlink_key: key,
                count,
                verdict: "resolved",
                reason_code: "supersession_resolved_sequence",
                effective_decided_at: Some(max_time),
                effective_decision: decision_value(seq_leaders[0]),
                detail: format!(
                    "equal decidedAt resolved by explicit (asserted) sequence {max_seq}"
                ),
            };
        }
        return ambiguous(
            key,
            count,
            "supersession_ambiguous_duplicate_sequence",
            "equal decidedAt and equal sequence; no deterministic winner",
        );
    }

    ambiguous(
        key,
        count,
        "supersession_ambiguous_missing_sequence",
        "equal decidedAt and no explicit ordering field; a nonce is not an ordering field",
    )
}

fn ambiguous(key: String, count: usize, reason_code: &'static str, detail: &str) -> GroupReport {
    GroupReport {
        backlink_key: key,
        count,
        verdict: "ambiguous",
        reason_code,
        effective_decided_at: None,
        effective_decision: None,
        detail: detail.to_string(),
    }
}

fn backlink_key(record: &Value) -> String {
    let backlink = record.get("backLink").or_else(|| record.get("back_link"));
    match backlink {
        Some(b) => format!(
            "attestationDigest={};attestationNonce={}",
            string_at(b, &["attestationDigest"])
                .or_else(|| string_at(b, &["attestation_digest"]))
                .unwrap_or_else(|| "-".to_string()),
            string_at(b, &["attestationNonce"])
                .or_else(|| string_at(b, &["attestation_nonce"]))
                .unwrap_or_else(|| "-".to_string()),
        ),
        None => "no-backlink".to_string(),
    }
}

fn decided_at(record: &Value) -> Option<String> {
    string_at(record, &["decisionDerived", "decidedAt"])
        .or_else(|| string_at(record, &["decision_derived", "decided_at"]))
}

fn decision_value(record: &Value) -> Option<String> {
    string_at(record, &["decisionDerived", "decision"])
        .or_else(|| string_at(record, &["decision_derived", "decision"]))
}

fn sequence(record: &Value) -> Option<i64> {
    record
        .get("decisionDerived")
        .or_else(|| record.get("decision_derived"))
        .and_then(|d| d.get("sequence"))
        .and_then(Value::as_i64)
}

fn string_at(value: &Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for segment in path {
        current = current.get(*segment)?;
    }
    current.as_str().map(ToOwned::to_owned)
}

fn print_table(report: &SupersessionReport) {
    println!("MCP Execution Record Supersession Report");
    println!("========================================");
    println!("OK:    {}", if report.ok { "yes" } else { "no" });
    println!("Role:  {}", report.verification_scope.role);
    println!();
    for group in &report.groups {
        println!(
            "{:<10} {:<44} count={} {}",
            group.verdict, group.reason_code, group.count, group.backlink_key
        );
        println!("           {}", group.detail);
    }
    println!();
    println!("Claims not made: {}", report.claims_not_made.join(", "));
}
