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

use super::effect_refutation::{refute_egress, EgressRefutation};
use crate::exit_codes;
use anyhow::{Context, Result};
use assay_evidence::bundle::BundleReader;
use assay_evidence::{
    coding_agent_claim_decision, CodingAgentClaimKind, CodingAgentCoverageState,
    CodingAgentGateDecision, CodingAgentSourceClass,
};
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

    /// An `assay.runner.observation_health.v0` artifact from a below-harness observer. When present,
    /// a call whose claimed egress was watched and not seen is REFUTED. Absent means no refutation
    /// is attempted, which is different from a refutation that failed.
    #[arg(long = "observation-health", value_name = "PATH")]
    pub observation_health: Option<PathBuf>,

    /// Peer endpoints the observer actually recorded for this workload, comma separated. Only read
    /// alongside --observation-health, because peers without a coverage descriptor cannot support
    /// an absence claim.
    #[arg(long = "observed-peer", value_delimiter = ',')]
    pub observed_peers: Vec<String>,

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
    /// What this level lets a consumer claim, from the existing claim gate rather than a second
    /// rule. `occurrence` asks "did this effect happen"; `bounded_negative` asks "did it not".
    occurrence_claim: CodingAgentGateDecision,
    bounded_negative_claim: CodingAgentGateDecision,
    /// What a below-harness observer could say about the claimed egress, when one was supplied.
    #[serde(skip_serializing_if = "Option::is_none")]
    egress: Option<EgressRefutation>,
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

/// The source class a side-effect level earns.
///
/// This is where Eb.5 composes rather than restates. The question "may this evidence carry an
/// occurrence claim about the world" is already answered by the coding-agent claim gate, and that
/// gate already encodes the occurrence-versus-absence asymmetry. Writing a second ladder-specific
/// rule here would be two implementations of one rule, which always drift.
///
/// The mapping is placed in the CLI on purpose: `assay-mcp-server` does not depend on
/// `assay-evidence` and must not start to for this. The CLI already depends on both, so composition
/// belongs here and a new crate edge stays an ADR question rather than a side effect of wiring.
fn source_class_for(level: SideEffectLevel) -> CodingAgentSourceClass {
    match level {
        // The provider said so. Nothing outside the producer corroborates it.
        SideEffectLevel::Asserted => CodingAgentSourceClass::ProducerReported,
        // Our own later read, at our own vantage. Real evidence, and still ours.
        SideEffectLevel::ObservedConfirmed => CodingAgentSourceClass::BoundaryObserved,
        // An independently produced record from the system that would know.
        SideEffectLevel::Verified => CodingAgentSourceClass::ThirdPartyObserved,
    }
}

/// What a level lets a consumer claim about the world.
///
/// Coverage is `SelfReported` for `asserted` and **`Partial`** above it.
///
/// Never `Observed`, and a test caught the first version getting this wrong. A side-effect level is
/// evidence about ONE CALL; it is never coverage of a dimension. `Observed` told the gate the whole
/// surface had been watched, which let `verified` license "and nothing else happened" — a claim an
/// audit record for a single call cannot support. `Partial` says exactly what is true: this
/// occurrence is corroborated, the dimension is not covered, so occurrence claims pass and absence
/// claims are blocked by `partial_coverage_blocks_absence_claim`.
fn claim_decision_for(
    level: SideEffectLevel,
    kind: CodingAgentClaimKind,
) -> CodingAgentGateDecision {
    let coverage = match level {
        SideEffectLevel::Asserted => CodingAgentCoverageState::SelfReported,
        _ => CodingAgentCoverageState::Partial,
    };
    coding_agent_claim_decision(source_class_for(level), coverage, kind).decision
}

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

    let observation_health: Option<Value> = match &args.observation_health {
        Some(path) => Some(
            serde_json::from_str(
                &std::fs::read_to_string(path)
                    .with_context(|| format!("cannot read {}", path.display()))?,
            )
            .with_context(|| format!("{} is not valid JSON", path.display()))?,
        ),
        None => None,
    };

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
                occurrence_claim: CodingAgentGateDecision::Blocked,
                bounded_negative_claim: CodingAgentGateDecision::Blocked,
                egress: None,
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
            // A below-harness observer contradicts the record only when it was demonstrably
            // watching. `refute_egress` refuses on every blind state rather than reading silence
            // as evidence, so an absent or degraded observer leaves the level untouched.
            if asserted {
                let expected = decision["action"]["target"]["provider"].as_str();
                row.egress = Some(refute_egress(
                    observation_health.as_ref(),
                    &args.observed_peers,
                    expected,
                ));
            }
            row.occurrence_claim =
                claim_decision_for(row.level, CodingAgentClaimKind::PositiveExistence);

            // A refutation overrides the ladder, including `verified`. If an imported audit record
            // says the call happened and a watching kernel observer says nothing left the cgroup,
            // those genuinely disagree, and the honest response is to block the occurrence claim and
            // show both rather than silently prefer whichever rung is higher. Preferring the audit
            // record would make the observer decorative; preferring the observer would let a probe
            // gap overturn real corroboration. The conflict is the finding.
            if row.egress.as_ref().is_some_and(EgressRefutation::refutes) {
                row.occurrence_claim = CodingAgentGateDecision::Blocked;
            }
            row.bounded_negative_claim =
                claim_decision_for(row.level, CodingAgentClaimKind::BoundedNegative);
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
                    "  {:40} asserted={:5} level={:18} occurrence={:?} absence={:?}",
                    c.tool,
                    c.asserted,
                    level.as_str().unwrap_or("?"),
                    c.occurrence_claim,
                    c.bounded_negative_claim
                );
                if let Some(e) = &c.egress {
                    println!("      egress: {}", serde_json::to_string(e)?);
                }
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
