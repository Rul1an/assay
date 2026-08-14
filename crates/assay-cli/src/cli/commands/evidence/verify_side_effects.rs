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
use crate::cli::commands::monitor::monitor_next::observed_peers::{
    ObservedPeers, OBSERVED_PEERS_SCHEMA,
};
use crate::exit_codes;
use anyhow::{Context, Result};
use assay_evidence::bundle::BundleReader;
use assay_evidence::{
    coding_agent_claim_decision, coding_agent_weakest_ceiling, CodingAgentClaimCeiling,
    CodingAgentClaimDecision, CodingAgentClaimKind, CodingAgentCoverageState,
    CodingAgentGateDecision, CodingAgentSourceClass, CodingAgentWeakestCeiling,
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

    /// An `assay.monitor.observed_peers.v0` artifact from the SAME run as --observation-health.
    ///
    /// Read from the artifact rather than typed on the command line, because a hand-supplied peer
    /// set makes the refutation only as good as what someone remembered to pass, and because peers
    /// from one run checked against another run's coverage would let a well-covered run vouch for a
    /// blind one. The run ids must match or nothing is refuted.
    #[arg(long = "observed-peers", value_name = "PATH")]
    pub observed_peers: Option<PathBuf>,

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
    /// How strong the occurrence claim is allowed to be, on the published ceiling ladder.
    ///
    /// The gate has always computed this and this report used to drop it, which made the ladder's
    /// ordering unobservable: `asserted` and `verified` both surfaced as a bare `Allowed`/`Degraded`
    /// and a consumer could not tell a provider's own word from an independent record. Carrying the
    /// rung is what makes `producer_reported < ... < independently_confirmed` mean something outside
    /// the type system.
    ///
    /// Absent on two kinds of row, and they are different. A call that asserted no side effect gets
    /// no rung because it made no occurrence claim to grade — publishing one there would attach a
    /// ladder position to a claim nobody made. And a call whose occurrence claim is `Blocked` gets
    /// none because a block is the statement that no rung applies. `asserted` distinguishes the two
    /// for a reader, which is why this stays an `Option` here while the run-level answer does not.
    #[serde(skip_serializing_if = "Option::is_none")]
    occurrence_ceiling: Option<CodingAgentClaimCeiling>,
    /// The full occurrence decision for this call, used by the ceiling fold. Kept alongside the
    /// serialized `occurrence_claim` and `occurrence_ceiling` so the fold consumes the decision
    /// directly (Blocker 3: no more `Option<Ceiling>` ambiguity).
    #[serde(skip)]
    occurrence_decision: CodingAgentClaimDecision,
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
    /// The strongest occurrence claim this report as a whole supports: the **weakest** rung across
    /// every call that asserted a side effect.
    ///
    /// A fold and never a maximum. One independently corroborated call does not raise what the run
    /// as a whole supports, and a reader who takes the strongest row away has learnt the wrong
    /// thing.
    ///
    /// Always emitted, and three-state rather than an `Option`, because an absent field would say
    /// two different things at once: that a refutation collapsed the run, and that no call in the
    /// bundle asserted anything. Those are the occurrence-versus-absence cases this command exists
    /// to keep apart, and the first draft of this field lost the distinction in the very field meant
    /// to carry it. The fold itself lives in `assay-evidence` and is shared with the coverage
    /// report's per-dimension version rather than copied.
    weakest_occurrence_ceiling: CodingAgentWeakestCeiling,
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
///
/// Returns the whole decision rather than just its verdict. An earlier version took `.decision` here
/// and threw the rung away, which left the ceiling ladder computed on every call and read by nobody
/// — a rule that runs but decides nothing. The caller now takes the part it needs.
fn claim_decision_for(
    level: SideEffectLevel,
    kind: CodingAgentClaimKind,
) -> CodingAgentClaimDecision {
    let coverage = match level {
        SideEffectLevel::Asserted => CodingAgentCoverageState::SelfReported,
        _ => CodingAgentCoverageState::Partial,
    };
    coding_agent_claim_decision(source_class_for(level), coverage, kind)
}

/// The weakest occurrence rung across the calls that asserted a side effect.
///
/// Only asserting calls count. A tool that never claimed to change anything outside the process has
/// nothing to corroborate, and folding its floor into the run's answer would report a weak run
/// because it did some reading. That filter is exactly why this set can be empty and why the shared
/// fold has to distinguish empty from blocked.
fn weakest_occurrence_ceiling(calls: &[CallRow]) -> CodingAgentWeakestCeiling {
    let asserting_decisions: Vec<&CodingAgentClaimDecision> = calls
        .iter()
        .filter(|c| c.asserted)
        .map(|c| &c.occurrence_decision)
        .collect();
    coding_agent_weakest_ceiling(asserting_decisions)
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

    // The peer set is only usable against the coverage descriptor of the run that produced it.
    let (observed_peers, peers_run_mismatch) = match (&args.observed_peers, &observation_health) {
        (Some(path), Some(oh)) => {
            // Deserialized into the producer's own type rather than read field by field, so a
            // malformed or mislabelled artifact fails here instead of silently yielding an empty
            // peer set that would look like "watched and saw nothing".
            let doc: ObservedPeers = serde_json::from_str(
                &std::fs::read_to_string(path)
                    .with_context(|| format!("cannot read {}", path.display()))?,
            )
            .with_context(|| {
                format!("{} is not a valid {OBSERVED_PEERS_SCHEMA}", path.display())
            })?;
            if doc.schema != OBSERVED_PEERS_SCHEMA {
                anyhow::bail!(
                    "{} declares schema {}, expected {OBSERVED_PEERS_SCHEMA}",
                    path.display(),
                    doc.schema
                );
            }
            let same_run = oh.get("run_id").and_then(Value::as_str) == Some(doc.run_id.as_str());
            (doc.peers, !same_run)
        }
        _ => (Vec::new(), false),
    };

    let records = match &args.audit_import {
        Some(dir) => load_audit_records(dir)?,
        None => Vec::new(),
    };

    // Require at least one decision surface event. A bundle with no decision events has nothing to
    // verify, and reporting an empty NothingClaimed would mask the fact that the bundle is not a
    // decision-surface bundle at all.
    let decision_events: Vec<_> = events
        .iter()
        .filter(|e| e.type_ == DECISION_EVENT_TYPE)
        .collect();
    if decision_events.is_empty() {
        anyhow::bail!(
            "bundle contains no {DECISION_EVENT_TYPE} events: \
             nothing to verify (supply a bundle that carries observed tool decisions)"
        );
    }

    let mut calls = Vec::new();
    let mut promoted = 0usize;
    let mut matched_records = vec![false; records.len()];

    for event in &decision_events {
        // Require observed_tool_decisions to be an array. The previous code silently continued on
        // missing/mistyped values, which turned a malformed bundle into an empty set and then
        // NothingClaimed — absence masquerading as clean.
        let decisions = event
            .payload
            .get("observed_tool_decisions")
            .with_context(|| {
                format!(
                    "{DECISION_EVENT_TYPE} event is missing observed_tool_decisions field \
                     (event id: {})",
                    event.id
                )
            })?
            .as_array()
            .with_context(|| {
                format!(
                    "{DECISION_EVENT_TYPE} event has observed_tool_decisions that is not an array \
                     (event id: {})",
                    event.id
                )
            })?;
        for decision in decisions {
            // Require side_effect_asserted to be a bool. The previous code defaulted to false on
            // missing/non-bool, which made a malformed response look like a call that asserted
            // nothing — the exact conflation this ladder exists to prevent.
            let asserted = decision
                .get("response")
                .and_then(|r| r.get("side_effect_asserted"))
                .with_context(|| {
                    format!(
                        "decision for tool {:?} is missing response.side_effect_asserted",
                        decision["tool"]["name"].as_str().unwrap_or("?")
                    )
                })?
                .as_bool()
                .with_context(|| {
                    format!(
                        "decision for tool {:?} has response.side_effect_asserted \
                         that is not a boolean",
                        decision["tool"]["name"].as_str().unwrap_or("?")
                    )
                })?;
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
                occurrence_ceiling: None,
                occurrence_decision: CodingAgentClaimDecision {
                    decision: CodingAgentGateDecision::Blocked,
                    ceiling: None,
                    gap: None,
                    rule: String::new(),
                },
                egress: None,
            };

            if asserted {
                // One-to-one allocation: skip records already matched to a prior call. Two
                // otherwise identical asserting calls each need their own audit record; letting
                // one record vouch for both would overcount corroboration.
                for (i, record) in records.iter().enumerate() {
                    if matched_records[i] {
                        continue;
                    }
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
                row.egress = Some(if peers_run_mismatch {
                    // Refusing here rather than comparing is the point: a peer set from a different
                    // run has no relationship to this run's coverage, and pairing them would be the
                    // borrowed-denominator version of the false GREEN.
                    EgressRefutation::NoCoverage {
                        reason: "observed_peers.run_id does not match observation_health.run_id: \
                                 a peer set only means anything against its own run's coverage",
                    }
                } else {
                    refute_egress(observation_health.as_ref(), &observed_peers, expected)
                });
            }
            let occurrence = claim_decision_for(row.level, CodingAgentClaimKind::PositiveExistence);
            row.occurrence_claim = occurrence.decision;
            // Only a call that asserted a side effect gets a rung. `occurrence_claim` is computed for
            // every row and that predates this change, but a ladder position is a stronger thing to
            // publish: it grades a claim, and a call that asserted nothing has not made one.
            row.occurrence_ceiling = occurrence.ceiling.filter(|_| row.asserted);

            // A refutation overrides the ladder, including `verified`. If an imported audit record
            // says the call happened and a watching kernel observer says nothing left the cgroup,
            // those genuinely disagree, and the honest response is to block the occurrence claim and
            // show both rather than silently prefer whichever rung is higher. Preferring the audit
            // record would make the observer decorative; preferring the observer would let a probe
            // gap overturn real corroboration. The conflict is the finding.
            if row.egress.as_ref().is_some_and(EgressRefutation::refutes) {
                row.occurrence_claim = CodingAgentGateDecision::Blocked;
                // The rung goes with it. A blocked claim that still advertised
                // `independently_confirmed` would let a reader take the number and drop the verdict,
                // which is the exact misreading the refutation exists to prevent.
                row.occurrence_ceiling = None;
            }

            // Carry the full decision for the ceiling fold. After a refutation override, rebuild
            // the decision so the fold sees the blocked state rather than the pre-refutation rung.
            row.occurrence_decision = CodingAgentClaimDecision {
                decision: row.occurrence_claim,
                ceiling: row.occurrence_ceiling,
                gap: occurrence.gap,
                rule: occurrence.rule,
            };
            row.bounded_negative_claim =
                claim_decision_for(row.level, CodingAgentClaimKind::BoundedNegative).decision;
            calls.push(row);
        }
    }

    let weakest_occurrence_ceiling = weakest_occurrence_ceiling(&calls);

    let report = Report {
        schema: "assay.side_effect_verification.v0",
        bundle: args.bundle.display().to_string(),
        audit_records_imported: records.len(),
        audit_records_unmatched: matched_records.iter().filter(|m| !**m).count(),
        calls,
        promoted,
        weakest_occurrence_ceiling,
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
            println!(
                "weakest occurrence ceiling: {}",
                match report.weakest_occurrence_ceiling {
                    CodingAgentWeakestCeiling::NothingClaimed =>
                        "nothing_claimed (no call asserted a side effect)".to_string(),
                    CodingAgentWeakestCeiling::Blocked =>
                        "blocked (an asserting call is contradicted or unsupported)".to_string(),
                    CodingAgentWeakestCeiling::Rung { ceiling } => format!("{ceiling:?}"),
                }
            );
            for c in &report.calls {
                let level = serde_json::to_value(c.level)?;
                println!(
                    "  {:40} asserted={:5} level={:18} occurrence={:?} ceiling={:?} absence={:?}",
                    c.tool,
                    c.asserted,
                    level.as_str().unwrap_or("?"),
                    c.occurrence_claim,
                    c.occurrence_ceiling,
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
