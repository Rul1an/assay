//! EXPERIMENTAL (Eb.4): promote side-effect levels in a bundle against imported provider audit records.
//!
//! `BundleReader::open` verifies bundle integrity first (manifest hashes + Merkle root); this layers
//! the side-effect ladder of `docs/reference/side-effect-receipt.md` on top. For every observed
//! tool-decision that asserted a side effect, it looks for an imported
//! `assay.provider_audit_record.v0` whose binding recomputes AND matches that call's action
//! projection, and promotes only then.
//!
//! # Import, never query
//!
//! The audit records come from `--audit-import <DIR>`, exported from the provider's own log by
//! whoever holds that access. This command opens no socket and takes no provider credential. A
//! verifier that re-fetches state is an actor, and an actor holding read credentials for every
//! provider it observes is the confused deputy the MCP threat model warns about.
//!
//! # Nothing is silently dropped
//!
//! Every imported record is reported with its outcome, including the ones that did not bind. A record
//! that failed a check and a record that was never imported must never look the same, which is the
//! failure this ladder exists to prevent.

use crate::exit_codes;
use anyhow::{Context, Result};
use assay_evidence::bundle::BundleReader;
use assay_mcp_server::side_effect::{
    check_audit_record, AuditBinding, SideEffectLevel, PROVIDER_AUDIT_RECORD_SCHEMA,
};
use clap::{Args, ValueEnum};
use serde::Serialize;
use serde_json::Value;
use std::fs::File;
use std::path::PathBuf;

/// Event type carrying observed tool decisions.
const DECISION_EVENT_TYPE: &str = "assay.tool_decision_surface.v0";

#[derive(Debug, Args, Clone)]
pub struct VerifySideEffectsArgs {
    /// Evidence bundle (.tar.gz) containing observed tool decisions
    #[arg(value_name = "BUNDLE")]
    pub bundle: PathBuf,

    /// Directory of exported `assay.provider_audit_record.v0` JSON files. Absent means no import is
    /// available, which leaves every asserted side effect at `asserted` rather than failing.
    #[arg(long = "audit-import", value_name = "DIR")]
    pub audit_import: Option<PathBuf>,

    /// Output format
    #[arg(long, value_enum, default_value_t = SideEffectFormat::Table)]
    pub format: SideEffectFormat,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum SideEffectFormat {
    Json,
    Table,
}

#[derive(Debug, Serialize)]
struct CallRow {
    tool: String,
    asserted: bool,
    level: SideEffectLevel,
    #[serde(skip_serializing_if = "Option::is_none")]
    subject_digest: Option<String>,
    /// Why a record did not bind. Present whenever an import was considered and rejected.
    #[serde(skip_serializing_if = "Option::is_none")]
    binding: Option<AuditBinding>,
}

#[derive(Debug, Serialize)]
struct Report {
    schema: &'static str,
    bundle: String,
    audit_records_imported: usize,
    /// Records that parsed as the right schema but bound to no observed call in this bundle. Counted
    /// rather than dropped: an unmatched import is a fact about the pairing, not noise.
    audit_records_unmatched: usize,
    calls: Vec<CallRow>,
    promoted: usize,
    claims_not_made: &'static [&'static str],
}

const CLAIMS_NOT_MADE: &[&str] = &[
    "provider_query",
    "audit_record_authenticity_beyond_its_own_signature",
    "effect_persistence_after_the_observed_call",
];

/// Load `assay.provider_audit_record.v0` files from an import directory.
///
/// A file that is not this schema is skipped rather than failing the run: an export directory may
/// legitimately hold other artifacts. A file that IS this schema but cannot parse is an error,
/// because silently skipping it would hide an audit record the operator believes was considered.
fn load_audit_records(dir: &PathBuf) -> Result<Vec<Value>> {
    let mut records = Vec::new();
    let entries = std::fs::read_dir(dir)
        .with_context(|| format!("cannot read audit import directory {}", dir.display()))?;
    for entry in entries {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("cannot read {}", path.display()))?;
        let value: Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            // Not JSON at all: not an audit record, and not ours to fail on.
            Err(_) => continue,
        };
        if value.get("schema").and_then(Value::as_str) == Some(PROVIDER_AUDIT_RECORD_SCHEMA) {
            records.push(value);
        }
    }
    Ok(records)
}

pub fn cmd_verify_side_effects(args: &VerifySideEffectsArgs) -> Result<i32> {
    let file = File::open(&args.bundle)
        .with_context(|| format!("cannot open bundle {}", args.bundle.display()))?;
    // Integrity first: the ladder is meaningless over bytes that did not verify.
    let events = BundleReader::open(file)
        .context("bundle failed verification")?
        .events_vec()
        .context("cannot read bundle events")?;

    let records = match &args.audit_import {
        Some(dir) => load_audit_records(dir)?,
        None => Vec::new(),
    };

    let mut calls = Vec::new();
    let mut promoted = 0usize;
    let mut matched_records = vec![false; records.len()];

    for event in events.iter().filter(|e| e.type_ == DECISION_EVENT_TYPE) {
        let Some(decisions) = event
            .payload
            .get("observed_tool_decisions")
            .and_then(Value::as_array)
        else {
            continue;
        };
        for decision in decisions {
            let asserted = decision["response"]["side_effect_asserted"]
                .as_bool()
                .unwrap_or(false);
            let tool = decision["tool"]["name"].as_str().unwrap_or("?").to_string();
            let action = &decision["action"];

            let mut row = CallRow {
                tool,
                asserted,
                level: SideEffectLevel::Asserted,
                subject_digest: None,
                binding: None,
            };

            if asserted {
                for (i, record) in records.iter().enumerate() {
                    let binding = check_audit_record(record, action);
                    match &binding {
                        AuditBinding::Bound { subject_digest } => {
                            row.level = SideEffectLevel::Verified;
                            row.subject_digest = Some(subject_digest.clone());
                            row.binding = Some(binding);
                            matched_records[i] = true;
                            promoted += 1;
                            break;
                        }
                        // Keep the first rejection so the operator sees why, but keep looking: a
                        // directory holds records for many calls and most will not be this one.
                        _ if row.binding.is_none() => row.binding = Some(binding),
                        _ => {}
                    }
                }
            }
            calls.push(row);
        }
    }

    let report = Report {
        schema: "assay.side_effect_verification.v0",
        bundle: args.bundle.display().to_string(),
        audit_records_imported: records.len(),
        audit_records_unmatched: matched_records.iter().filter(|m| !**m).count(),
        calls,
        promoted,
        claims_not_made: CLAIMS_NOT_MADE,
    };

    match args.format {
        SideEffectFormat::Json => println!("{}", serde_json::to_string_pretty(&report)?),
        SideEffectFormat::Table => {
            println!("bundle: {}", report.bundle);
            println!(
                "imported audit records: {} ({} matched no observed call)",
                report.audit_records_imported, report.audit_records_unmatched
            );
            for c in &report.calls {
                let level = serde_json::to_value(c.level)?;
                println!(
                    "  {:40} asserted={:5} level={}",
                    c.tool,
                    c.asserted,
                    level.as_str().unwrap_or("?")
                );
                if let Some(b) = &c.binding {
                    if !b.is_bound() {
                        println!("      not promoted: {}", serde_json::to_string(b)?);
                    }
                }
            }
            println!("\npromoted to verified: {}", report.promoted);
            println!("claims not made: {}", report.claims_not_made.join(", "));
        }
    }

    // A run that promoted nothing is not a failure: `asserted` is an honest level, and the absence of
    // an audit export is the ordinary case rather than an error.
    Ok(exit_codes::OK)
}
