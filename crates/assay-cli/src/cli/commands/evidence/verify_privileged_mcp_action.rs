//! EXPERIMENTAL: verify an evidence bundle against privileged-mcp-action v0 or v1.
//!
//! Spec: `docs/profiles/privileged-mcp-action/v0.md`. Four byte-pure stages:
//! 1. bundle integrity (the shipped bundle verification, via `BundleReader::open` which runs
//!    `verify_bundle_with_limits`; any failure means `bundle_integrity: "fail"` and nothing below
//!    stage 1 is consumed);
//! 2. statement well-formedness (cardinality, closed vocabularies, producer non-claims verbatim);
//! 3. binding validity (marker triple + exact `(tool name, target digest)` byte equality);
//! 4. claim recompute (the fixed matrix; establish records are diagnostic journey only).
//!
//! The output is a claim matrix, never a score. Refuted claims still exit 0: the report is the
//! product and consumers gate on cells; only integrity failure or an invalid verdict exits 2.

use crate::evidence_verify_reason::{
    reason_code_for_evidence_error, PROFILE_EVIDENCE_REASON_CODES,
};
use crate::exit_codes::{self, ReasonCode};
use anyhow::{Context, Result};
use assay_evidence::bundle::BundleReader;
use assay_evidence::denial_marker::{classify_denial_marker, DenialMarkerVersion};
use assay_evidence::types::EvidenceEvent;
use clap::{Args, ValueEnum};
use serde::Serialize;
use serde_json::Value;
use std::fs::File;
use std::path::{Path, PathBuf};

const REPORT_SCHEMA: &str = "assay.privileged_mcp_action.verify.report.v0";
const PROFILE_ID: &str = "privileged-mcp-action/v0";
const PROFILE_ID_V1: &str = "privileged-mcp-action/v1";

const DECISION_SCHEMA: &str = "assay.enforcement_decision.v0";
const OBSERVATION_SCHEMA: &str = "assay.denied_call_observation.v0";
const OBSERVATION_SCHEMA_V1: &str = "assay.denied_call_observation.v1";
const ESTABLISH_SCHEMA: &str = "assay.manifest_establish.v0";
/// Profile namespaces: any payload schema with one of these prefixes that is not a recognized
/// record for the *selected* profile version fails closed. Selection is explicit (`--profile-version`);
/// there is no autodetect.
const PROFILE_NAMESPACES: &[&str] = &[
    "assay.enforcement_decision.",
    "assay.denied_call_observation.",
    "assay.manifest_establish.",
];

const DECISION_VOCAB: &[&str] = &["allow", "deny"];
const REASON_VOCAB: &[&str] = &[
    "unclassified_tool_call",
    "classification_incomplete",
    "no_declared_allowance",
    "credential_scope_unknown",
    "credential_scope_insufficient",
    "manifest_baseline_missing",
    "manifest_observation_ambiguous",
    "manifest_current_observation_incomplete",
    "manifest_drifted_since_approval",
    "allow",
];
const DRIFT_STATE_VOCAB: &[&str] = &[
    "satisfied",
    "baseline_missing",
    "current_observation_incomplete",
    "observation_ambiguous",
    "drifted",
    "not_evaluated",
];
const ESTABLISH_PATH_VOCAB: &[&str] = &[
    "no_establish_needed",
    "established_then_allowed",
    "established_then_denied",
    "immediate_deny",
];

// Marker recognition is the shared classifier in `assay_evidence::denial_marker`.

/// The producer's five decision non-claims, byte-exact (source of truth:
/// `assay-mcp-server/src/proxy/enforce/records.rs`; the fourth carries U+2014).
const DECISION_PRODUCER_NON_CLAIMS: &[&str] = &[
    "policy decision only; does not assert or verify the upstream side effect (stays asserted, E9 ladder)",
    "an allow is the decision to forward; it does not assert the call reached or was performed by the upstream (a transport failure surfaces as proxy_failed, not here)",
    "credential referenced by alias only, never the token or declared scopes",
    "deny is fail-closed caution and allow is a policy decision \u{2014} neither is a maliciousness verdict",
    "not the observation artifact (assay.mcp_manifest_observed.v0) and not the mechanism artifact (assay.enforcement_health.v0)",
];

/// The producer's four observation non-claims, byte-exact (source of truth:
/// `assay-mcp-server/src/proxy/denied_observation.rs`).
const OBSERVATION_PRODUCER_NON_CLAIMS: &[&str] = &[
    "caller-visible proxy denial observation only; policy decision lives in assay.enforcement_decision.v0",
    "does not assert or verify the upstream side effect",
    "does not assert maliciousness, safety, approval, or whole-action trust",
    "must not be read as a replacement for the bound enforcement decision record",
];

/// The four fixed report non-claims, verbatim from the spec, present in every report.
const REPORT_NON_CLAIMS: [&str; 4] = [
    "allow does not prove upstream delivery",
    "deny does not establish maliciousness",
    "caller-visible denial does not prove external side-effect absence",
    "bundle integrity does not upgrade source class",
];

#[derive(Debug, Args, Clone)]
pub struct VerifyPrivilegedMcpActionArgs {
    /// Evidence bundle (.tar.gz) carrying privileged-mcp-action profile records
    #[arg(value_name = "BUNDLE")]
    pub bundle: PathBuf,

    /// Output format
    #[arg(long, value_enum, default_value_t = VerifyFormat::Table)]
    pub format: VerifyFormat,

    /// Selected privileged-mcp-action interpreter. Omit for default v0; passing
    /// the flag is explicit. No autodetect from bundle contents. Does not name
    /// a carried input profile id (v0/v1 declare none).
    #[arg(long, value_enum)]
    pub profile_version: Option<ProfileVersion>,
}

#[derive(Debug, Clone, Copy, Default, ValueEnum, PartialEq, Eq)]
pub enum ProfileVersion {
    #[default]
    #[value(name = "v0")]
    V0,
    #[value(name = "v1")]
    V1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProfileSelection {
    Default,
    Explicit,
}

impl ProfileSelection {
    fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Explicit => "explicit",
        }
    }
}

struct ProfileProjection {
    profile: &'static str,
    profile_selection: ProfileSelection,
    input_profile: Option<&'static str>,
    input_profile_status: &'static str,
}

/// One projection rule: selected interpreter is not input identity.
/// Frozen v0/v1 carry no profile id; do not infer one from `--profile-version`.
fn project_profile(
    selected: ProfileVersion,
    profile_selection: ProfileSelection,
) -> ProfileProjection {
    ProfileProjection {
        profile: selected.as_profile_id(),
        profile_selection,
        input_profile: None,
        input_profile_status: "undeclared_legacy",
    }
}

impl ProfileVersion {
    fn as_profile_id(self) -> &'static str {
        match self {
            Self::V0 => PROFILE_ID,
            Self::V1 => PROFILE_ID_V1,
        }
    }

    fn observation_schema(self) -> &'static str {
        match self {
            Self::V0 => OBSERVATION_SCHEMA,
            Self::V1 => OBSERVATION_SCHEMA_V1,
        }
    }

    fn marker_version(self) -> DenialMarkerVersion {
        match self {
            Self::V0 => DenialMarkerVersion::V0,
            Self::V1 => DenialMarkerVersion::V1,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum VerifyFormat {
    Json,
    Table,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub schema: &'static str,
    /// Selected interpreter. Never detected or carried input identity.
    pub profile: &'static str,
    pub profile_selection: ProfileSelection,
    /// Frozen v0/v1 always serialize JSON null.
    pub input_profile: Option<&'static str>,
    pub input_profile_status: &'static str,
    pub bundle_integrity: &'static str,
    /// Present only when `bundle_integrity` is `pass`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verdict: Option<&'static str>,
    /// Present only when `verdict` is `valid`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claims: Option<Claims>,
    pub findings: Vec<Finding>,
    pub non_claims: [&'static str; 4],
    /// Verifier diagnostic outside the claim lattice. Absent on success.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<&'static str>,
    /// Shell-free remediation. Unreadable I/O uses caller-argv JSON; other
    /// codes stay prose. Absent on success.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_step: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Claims {
    pub policy_decision_recorded: ClaimCell,
    pub caller_visible_denial: ClaimCell,
    pub upstream_delivery: ClaimCell,
    pub external_side_effect: ClaimCell,
}

#[derive(Debug, Serialize)]
pub struct ClaimCell {
    pub status: &'static str,
    /// Only confirmed and refuted cells carry a source class; incomplete cells carry none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_class: Option<&'static str>,
}

impl ClaimCell {
    fn confirmed() -> Self {
        Self {
            status: "confirmed",
            source_class: Some("producer_reported"),
        }
    }
    fn refuted() -> Self {
        Self {
            status: "refuted",
            source_class: Some("producer_reported"),
        }
    }
    fn incomplete() -> Self {
        Self {
            status: "incomplete",
            source_class: None,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Finding {
    pub id: String,
    pub detail: String,
    /// Exact in-namespace payload schema when one was observed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed_schema: Option<String>,
}

pub fn cmd_verify_privileged_mcp_action(args: VerifyPrivilegedMcpActionArgs) -> Result<i32> {
    let profile_selection = match args.profile_version {
        None => ProfileSelection::Default,
        Some(_) => ProfileSelection::Explicit,
    };
    let profile_version = args.profile_version.unwrap_or(ProfileVersion::V0);
    let report = verify_bundle_report_for(&args.bundle, profile_version, profile_selection);
    match args.format {
        VerifyFormat::Json => println!("{}", serde_json::to_string_pretty(&report)?),
        VerifyFormat::Table => print_table(&report),
    }
    let ok = report.bundle_integrity == "pass" && report.verdict == Some("valid");
    Ok(if ok { exit_codes::OK } else { 2 })
}

/// Stage 1 + stages 2-4 over one bundle path. Open and read failures are the same
/// stage-1 report: never a usage-error envelope. Typed `VerifyError` is
/// authoritative; untyped I/O (including a directory path) is Unreadable.
/// `findings.detail` may retain the caller argv path. Unreadable `next_step` is
/// shell-free caller-argv via `ReasonCode::next_step`, not a second remediation.
#[cfg(test)]
pub fn verify_bundle_report(bundle: &Path) -> Report {
    verify_bundle_report_for(bundle, ProfileVersion::V0, ProfileSelection::Default)
}

pub fn verify_bundle_report_for(
    bundle: &Path,
    profile_version: ProfileVersion,
    profile_selection: ProfileSelection,
) -> Report {
    match read_bundle_events(bundle) {
        Ok(events) => profile_report_for_selection(&events, profile_version, profile_selection),
        Err(err) => integrity_fail_report(&err, bundle, profile_version, profile_selection),
    }
}

fn read_bundle_events(bundle: &Path) -> anyhow::Result<Vec<EvidenceEvent>> {
    let file = File::open(bundle)
        .with_context(|| format!("failed to open bundle {}", bundle.display()))?;
    // Stage 1: BundleReader::open runs the full shipped bundle verification
    // (verify_bundle_with_limits with default limits) before exposing any event.
    // Context names the caller argv without replacing a typed VerifyError in the chain.
    let reader = BundleReader::open(file)
        .with_context(|| format!("failed to read bundle {}", bundle.display()))?;
    reader
        .events_vec()
        .with_context(|| format!("failed to read events from bundle {}", bundle.display()))
}

struct ReportBody {
    bundle_integrity: &'static str,
    verdict: Option<&'static str>,
    claims: Option<Claims>,
    findings: Vec<Finding>,
    reason_code: Option<&'static str>,
    next_step: Option<String>,
}

fn emit_report(
    profile_version: ProfileVersion,
    profile_selection: ProfileSelection,
    body: ReportBody,
) -> Report {
    let projected = project_profile(profile_version, profile_selection);
    Report {
        schema: REPORT_SCHEMA,
        profile: projected.profile,
        profile_selection: projected.profile_selection,
        input_profile: projected.input_profile,
        input_profile_status: projected.input_profile_status,
        bundle_integrity: body.bundle_integrity,
        verdict: body.verdict,
        claims: body.claims,
        findings: body.findings,
        non_claims: REPORT_NON_CLAIMS,
        reason_code: body.reason_code,
        next_step: body.next_step,
    }
}

fn stage1_fail_report(
    detail: String,
    reason: ReasonCode,
    bundle: &Path,
    profile_version: ProfileVersion,
    profile_selection: ProfileSelection,
) -> Report {
    // Caller argv is safe to republish shell-free, unlike discovered host paths.
    emit_report(
        profile_version,
        profile_selection,
        ReportBody {
            bundle_integrity: "fail",
            verdict: None,
            claims: None,
            findings: vec![Finding {
                id: "bundle_integrity".to_string(),
                detail,
                observed_schema: None,
            }],
            reason_code: Some(reason.as_str()),
            next_step: Some(reason.next_step(bundle.to_str())),
        },
    )
}

fn integrity_fail_report(
    err: &anyhow::Error,
    bundle: &Path,
    profile_version: ProfileVersion,
    profile_selection: ProfileSelection,
) -> Report {
    let reason = reason_code_for_evidence_error(err)
        .expect("stage-1 evidence errors must map to a ReasonCode");
    debug_assert!(
        PROFILE_EVIDENCE_REASON_CODES
            .iter()
            .any(|(_, owned)| *owned == reason),
        "stage-1 reason must be one of the binary-owned profile codes"
    );
    stage1_fail_report(
        format!("{err:#}"),
        reason,
        bundle,
        profile_version,
        profile_selection,
    )
}

/// Stages 2-4 over the events of a bundle that already passed stage 1.
/// Default profile is v0 (explicit selection lives on `profile_report_for_selection`).
#[cfg(test)]
pub fn profile_report(events: &[EvidenceEvent]) -> Report {
    profile_report_for(events, ProfileVersion::V0)
}

/// Stages 2-4 for a selected profile version. No autodetect.
/// Unit tests that do not go through clap use `ProfileSelection::Default`.
#[cfg(test)]
pub fn profile_report_for(events: &[EvidenceEvent], profile_version: ProfileVersion) -> Report {
    profile_report_for_selection(events, profile_version, ProfileSelection::Default)
}

fn profile_report_for_selection(
    events: &[EvidenceEvent],
    profile_version: ProfileVersion,
    profile_selection: ProfileSelection,
) -> Report {
    let mut violations: Vec<Finding> = Vec::new();

    // Select profile events by the exact schema member of their payload.
    let mut decisions: Vec<&Value> = Vec::new();
    let mut observations: Vec<&Value> = Vec::new();
    let mut establishes: Vec<&Value> = Vec::new();
    for ev in events {
        let schema = ev.payload.get("schema").and_then(Value::as_str);
        match schema {
            Some(s) if in_namespace(s) => {
                if ev.type_ != s {
                    violations.push(finding(
                        "event_type_schema_mismatch",
                        format!(
                            "event {} has type {:?} but its payload declares schema {s:?}; the two must be equal",
                            ev.id, ev.type_
                        ),
                    ));
                }
                match s {
                    DECISION_SCHEMA => decisions.push(&ev.payload),
                    ESTABLISH_SCHEMA => establishes.push(&ev.payload),
                    other if other == profile_version.observation_schema() => {
                        observations.push(&ev.payload)
                    }
                    other => violations.push(unknown_schema_finding(
                        other,
                        format!(
                            "payload schema {other:?} is inside the profile namespace but is not a recognized {} record; unknown fails closed",
                            profile_version.as_profile_id()
                        ),
                    )),
                }
            }
            // In-namespace event TYPE without an in-namespace payload schema: the profile selects
            // by the payload's schema member, so an envelope claiming a profile type that its
            // payload does not declare is not an ignorable outside-the-profile event. Fail closed.
            _ if in_namespace(&ev.type_) => violations.push(finding(
                "unknown_profile_schema",
                format!(
                    "event {} has profile-namespace type {:?} but its payload does not declare that profile schema",
                    ev.id, ev.type_
                ),
            )),
            _ => {} // Outside the profile namespace: ignored (integrity still covered by stage 1).
        }
    }

    // Stage 2: cardinality.
    if decisions.len() != 1 {
        violations.push(finding(
            "decision_cardinality",
            format!(
                "exactly one assay.enforcement_decision.v0 payload is required, found {}; v0 is single-call by design",
                decisions.len()
            ),
        ));
    }
    if observations.len() > 1 {
        violations.push(finding(
            "observation_cardinality",
            format!(
                "at most one {} payload is allowed, found {}",
                profile_version.observation_schema(),
                observations.len()
            ),
        ));
    }
    if establishes.len() > 1 {
        violations.push(finding(
            "establish_cardinality",
            format!(
                "at most one assay.manifest_establish.v0 payload is allowed, found {}",
                establishes.len()
            ),
        ));
    }

    // Stage 2: record constraints (only meaningful on the single instance of each record kind).
    let decision = (decisions.len() == 1).then(|| decisions[0]);
    let observation = (observations.len() == 1).then(|| observations[0]);
    let establish = (establishes.len() == 1).then(|| establishes[0]);

    if let Some(dec) = decision {
        check_decision(dec, &mut violations);
    }
    if let Some(obs) = observation {
        check_observation(obs, &mut violations);
    }
    if let Some(est) = establish {
        check_establish(est, &mut violations);
    }

    // Stage 3: binding validity. An observation participates as a caller-visible denial marker only
    // if the shared classifier returns the selected profile's exact triple.
    let marker = observation
        .filter(|obs| classify_denial_marker(obs) == Some(profile_version.marker_version()));
    let mut bound_marker = false;
    if let Some(obs) = marker {
        let obs_tool = obs
            .get("call")
            .and_then(|c| c.get("tool_name"))
            .and_then(Value::as_str);
        let obs_digest = obs
            .get("call")
            .and_then(|c| c.get("target_digest"))
            .and_then(Value::as_str)
            .filter(|d| !d.is_empty());
        match (decision, obs_digest) {
            (None, _) => violations.push(finding(
                "marker_not_backed",
                "a caller-visible denial marker is present with no decision record to back it"
                    .to_string(),
            )),
            (Some(_), None) => violations.push(finding(
                "marker_digest_unbindable",
                "the marker's call.target_digest is null or empty, so it cannot bind to any decision"
                    .to_string(),
            )),
            (Some(dec), Some(obs_digest)) => {
                let dec_tool = dec
                    .get("tool")
                    .and_then(|t| t.get("name"))
                    .and_then(Value::as_str);
                let dec_digest = dec
                    .get("action")
                    .and_then(|a| a.get("target_digest"))
                    .and_then(Value::as_str);
                // Exact byte equality on both binding members; digests are never re-encoded.
                if obs_tool.is_some() && obs_tool == dec_tool && Some(obs_digest) == dec_digest {
                    bound_marker = true;
                } else {
                    violations.push(finding(
                        "observation_binding",
                        "the marker's (call.tool_name, call.target_digest) does not equal the decision's (tool.name, action.target_digest)"
                            .to_string(),
                    ));
                }
            }
        }
    }

    if !violations.is_empty() {
        return emit_report(
            profile_version,
            profile_selection,
            ReportBody {
                bundle_integrity: "pass",
                verdict: Some("invalid"),
                claims: None,
                findings: violations,
                reason_code: Some(ReasonCode::EEvidenceProfileInvalid.as_str()),
                next_step: Some(ReasonCode::EEvidenceProfileInvalid.next_step(None)),
            },
        );
    }

    // Stage 4: claim recompute. From here the records are validated; the matrix is a deterministic
    // function of (decision, bound marker), and the establish record changes no cell.
    let mut notes: Vec<Finding> = Vec::new();
    let dec = decision.expect("valid verdict implies exactly one decision");
    let decided = dec
        .get("decision")
        .and_then(Value::as_str)
        .expect("valid verdict implies vocabulary-checked decision");

    let caller_visible_denial = match (decided, bound_marker) {
        ("deny", true) => ClaimCell::confirmed(),
        ("deny", false) | ("allow", false) => ClaimCell::incomplete(),
        ("allow", true) => {
            notes.push(finding(
                "caller_visible_outcome_contradiction",
                "the producer recorded an allow decision and a caller-visible denial for the same (tool, target); the caller-visible outcome is refuted"
                    .to_string(),
            ));
            ClaimCell::refuted()
        }
        _ => unreachable!("decision vocabulary is closed"),
    };

    if let Some(est) = establish {
        let path = est
            .get("establish_path")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let contradiction = match decided {
            "allow" => matches!(path, "immediate_deny" | "established_then_denied"),
            "deny" => path == "established_then_allowed",
            _ => false,
        };
        if contradiction {
            notes.push(finding(
                "establish_journey_contradiction",
                format!(
                    "establish_path {path:?} contradicts the recorded {decided:?} decision; the journey is diagnostic only and the matrix is unchanged"
                ),
            ));
        }
    }

    emit_report(
        profile_version,
        profile_selection,
        ReportBody {
            bundle_integrity: "pass",
            verdict: Some("valid"),
            claims: Some(Claims {
                policy_decision_recorded: ClaimCell::confirmed(),
                caller_visible_denial,
                upstream_delivery: ClaimCell::incomplete(),
                external_side_effect: ClaimCell::incomplete(),
            }),
            findings: notes,
            reason_code: None,
            next_step: None,
        },
    )
}

fn check_decision(dec: &Value, violations: &mut Vec<Finding>) {
    check_vocab(dec, "decision", DECISION_VOCAB, violations);
    check_vocab(dec, "reason", REASON_VOCAB, violations);
    check_vocab(dec, "drift_state", DRIFT_STATE_VOCAB, violations);

    let tool_name = dec
        .get("tool")
        .and_then(|t| t.get("name"))
        .and_then(Value::as_str);
    if tool_name.map(str::is_empty).unwrap_or(true) {
        violations.push(finding(
            "decision_tool_name",
            "decision tool.name must be a non-empty string".to_string(),
        ));
    }

    let digest = dec
        .get("action")
        .and_then(|a| a.get("target_digest"))
        .and_then(Value::as_str);
    if !digest.map(is_sha256_digest).unwrap_or(false) {
        violations.push(finding(
            "target_digest_missing",
            "decision action.target_digest must be sha256:<64 lowercase hex>; a null digest cannot be bound and falls outside the profile"
                .to_string(),
        ));
    }

    // fail_closed is derived by the producer (true iff deny); divergence is malformation, not an
    // alternative policy statement. Only checkable once the decision value itself is in vocabulary.
    match (
        dec.get("decision").and_then(Value::as_str),
        dec.get("fail_closed").and_then(Value::as_bool),
    ) {
        (Some(d), Some(fc)) if DECISION_VOCAB.contains(&d) => {
            if fc != (d == "deny") {
                violations.push(finding(
                    "fail_closed_derivation",
                    format!("fail_closed is {fc} but the decision is {d:?}; fail_closed must equal (decision == \"deny\")"),
                ));
            }
        }
        (Some(d), None) if DECISION_VOCAB.contains(&d) => violations.push(finding(
            "fail_closed_derivation",
            "decision fail_closed must be a boolean".to_string(),
        )),
        _ => {} // The decision-vocabulary violation is already recorded.
    }

    check_non_claims(
        dec,
        DECISION_PRODUCER_NON_CLAIMS,
        "decision_non_claims",
        violations,
    );
}

fn check_observation(obs: &Value, violations: &mut Vec<Finding>) {
    let tool_name = obs
        .get("call")
        .and_then(|c| c.get("tool_name"))
        .and_then(Value::as_str);
    if tool_name.map(str::is_empty).unwrap_or(true) {
        violations.push(finding(
            "observation_tool_name",
            "observation call.tool_name must be a non-empty string".to_string(),
        ));
    }

    let error = obs.get("caller_visible_error");
    for member in ["code", "origin", "reason"] {
        let present = error
            .and_then(|e| e.get(member))
            .map(|v| !v.is_null())
            .unwrap_or(false);
        if !present {
            violations.push(finding(
                "observation_error_member",
                format!("observation caller_visible_error.{member} must be present"),
            ));
        }
    }

    let digest = obs
        .get("caller_visible_response_digest")
        .and_then(Value::as_str);
    if !digest.map(is_sha256_digest).unwrap_or(false) {
        violations.push(finding(
            "observation_response_digest",
            "observation caller_visible_response_digest must be sha256:<64 lowercase hex>"
                .to_string(),
        ));
    }

    check_non_claims(
        obs,
        OBSERVATION_PRODUCER_NON_CLAIMS,
        "observation_non_claims",
        violations,
    );
}

fn check_establish(est: &Value, violations: &mut Vec<Finding>) {
    check_vocab(est, "establish_path", ESTABLISH_PATH_VOCAB, violations);

    // establish_attempted must equal (run_outcome != "not_performed"); both members are required
    // for the equality to be checkable, so a missing member fails closed.
    match (
        est.get("establish_attempted").and_then(Value::as_bool),
        est.get("run_outcome").and_then(Value::as_str),
    ) {
        (Some(attempted), Some(outcome)) => {
            if attempted != (outcome != "not_performed") {
                violations.push(finding(
                    "establish_attempted_derivation",
                    format!(
                        "establish_attempted is {attempted} but run_outcome is {outcome:?}; establish_attempted must equal (run_outcome != \"not_performed\")"
                    ),
                ));
            }
        }
        _ => violations.push(finding(
            "establish_attempted_derivation",
            "establish record must carry boolean establish_attempted and string run_outcome"
                .to_string(),
        )),
    }
}

fn check_vocab(record: &Value, member: &str, vocab: &[&str], violations: &mut Vec<Finding>) {
    match record.get(member).and_then(Value::as_str) {
        Some(value) if vocab.contains(&value) => {}
        Some(value) => violations.push(finding(
            &format!("{member}_vocabulary"),
            format!(
                "{member} {value:?} is outside the closed set {}",
                vocab.join("|")
            ),
        )),
        None => violations.push(finding(
            &format!("{member}_vocabulary"),
            format!(
                "{member} must be a string from the closed set {}",
                vocab.join("|")
            ),
        )),
    }
}

/// Subset test: the record's `non_claims` array must contain every producer string verbatim
/// (byte-exact); extra entries are allowed.
fn check_non_claims(record: &Value, required: &[&str], id: &str, violations: &mut Vec<Finding>) {
    let Some(array) = record.get("non_claims").and_then(Value::as_array) else {
        violations.push(finding(
            id,
            "non_claims must be an array carrying the producer strings verbatim".to_string(),
        ));
        return;
    };
    for required_entry in required {
        let present = array.iter().any(|v| v.as_str() == Some(*required_entry));
        if !present {
            violations.push(finding(
                id,
                format!("non_claims is missing the producer string {required_entry:?}"),
            ));
        }
    }
}

fn in_namespace(schema: &str) -> bool {
    PROFILE_NAMESPACES.iter().any(|p| schema.starts_with(p))
}

fn is_sha256_digest(value: &str) -> bool {
    match value.strip_prefix("sha256:") {
        Some(hex) => hex.len() == 64 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')),
        None => false,
    }
}

fn finding(id: &str, detail: String) -> Finding {
    Finding {
        id: id.to_string(),
        detail,
        observed_schema: None,
    }
}

fn unknown_schema_finding(observed_schema: &str, detail: String) -> Finding {
    Finding {
        id: "unknown_profile_schema".to_string(),
        detail,
        observed_schema: Some(observed_schema.to_string()),
    }
}

fn print_table(report: &Report) {
    println!("Privileged MCP Action Verification ({})", report.profile);
    println!("=====================================================");
    println!("Profile selection: {}", report.profile_selection.as_str());
    println!(
        "Input profile:     {} ({})",
        report.input_profile_status,
        match report.input_profile {
            None => "none",
            Some(id) => id,
        }
    );
    println!("Bundle integrity: {}", report.bundle_integrity);
    if let Some(verdict) = report.verdict {
        println!("Verdict:          {verdict}");
    }
    if let Some(reason) = report.reason_code {
        println!("Reason code:      {reason}");
    }
    if let Some(next_step) = &report.next_step {
        println!("Next step:        {next_step}");
    }
    if let Some(claims) = &report.claims {
        println!();
        println!("Claim matrix:");
        for (name, cell) in [
            ("policy_decision_recorded", &claims.policy_decision_recorded),
            ("caller_visible_denial", &claims.caller_visible_denial),
            ("upstream_delivery", &claims.upstream_delivery),
            ("external_side_effect", &claims.external_side_effect),
        ] {
            match cell.source_class {
                Some(sc) => println!("  {name:<26} {:<10} ({sc})", cell.status),
                None => println!("  {name:<26} {}", cell.status),
            }
        }
    }
    println!();
    if report.findings.is_empty() {
        println!("Findings: none");
    } else {
        println!("Findings:");
        for f in &report.findings {
            println!("  {:<36} {}", f.id, f.detail);
        }
    }
    println!();
    println!("Non-claims:");
    for nc in &report.non_claims {
        println!("  - {nc}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const TOOL: &str = "github.add_deploy_key";
    const DIGEST: &str = "sha256:c3ff823d7fb2ee33b9f1a3f7be6eaf849acb980b6ec960731506436b56384dfc";

    fn decision_payload(decision: &str) -> Value {
        json!({
            "schema": DECISION_SCHEMA,
            "caller": {"id": "ci-agent"},
            "tool": {"name": TOOL, "action_class": "github_deploy_key"},
            "action": {
                "verb": "create",
                "resource_type": "github_deploy_key",
                "target": {"provider": "github", "owner": "acme", "repo": "prod-app"},
                "target_digest": DIGEST,
            },
            "decision": decision,
            "reason": if decision == "allow" { "allow" } else { "no_declared_allowance" },
            "fail_closed": decision == "deny",
            "drift_state": if decision == "allow" { "satisfied" } else { "not_evaluated" },
            "credential_alias": "gh-deploy",
            "non_claims": DECISION_PRODUCER_NON_CLAIMS,
        })
    }

    fn observation_payload() -> Value {
        json!({
            "schema": OBSERVATION_SCHEMA,
            "call": {"tool_name": TOOL, "target_digest": DIGEST},
            "caller_visible_error": {
                "code": -32042,
                "origin": "assay-proxy",
                "reason": "no_declared_allowance",
            },
            "caller_visible_response_digest":
                "sha256:ef5796d82cdedf4727caa82604687648c081271ac60e355a57089550a431f48b",
            "non_claims": OBSERVATION_PRODUCER_NON_CLAIMS,
        })
    }

    fn observation_payload_v1() -> Value {
        let mut obs = observation_payload();
        obs["schema"] = json!(OBSERVATION_SCHEMA_V1);
        obs["caller_visible_error"]["code"] = json!(-31999);
        obs
    }

    fn establish_payload(path: &str) -> Value {
        json!({
            "schema": ESTABLISH_SCHEMA,
            "establish_path": path,
            "establish_attempted": true,
            "action_class": "github_deploy_key",
            "run_outcome": "complete",
        })
    }

    fn ev(payload: Value, seq: u64) -> EvidenceEvent {
        let type_ = payload
            .get("schema")
            .and_then(Value::as_str)
            .unwrap()
            .to_string();
        EvidenceEvent::new(type_, "urn:assay:test", "run", seq, payload)
    }

    fn status(report: &Report, claim: &str) -> String {
        let claims = serde_json::to_value(report.claims.as_ref().unwrap()).unwrap();
        claims[claim]["status"].as_str().unwrap().to_string()
    }

    #[test]
    fn deny_with_bound_marker_confirms_both_confirmable_claims() {
        let report = profile_report(&[
            ev(decision_payload("deny"), 0),
            ev(observation_payload(), 1),
        ]);
        assert_eq!(report.verdict, Some("valid"));
        assert_eq!(status(&report, "policy_decision_recorded"), "confirmed");
        assert_eq!(status(&report, "caller_visible_denial"), "confirmed");
        assert_eq!(status(&report, "upstream_delivery"), "incomplete");
        assert_eq!(status(&report, "external_side_effect"), "incomplete");
        assert!(report.findings.is_empty());
    }

    #[test]
    fn allow_with_bound_marker_refutes_caller_visible_outcome() {
        let report = profile_report(&[
            ev(decision_payload("allow"), 0),
            ev(observation_payload(), 1),
        ]);
        assert_eq!(report.verdict, Some("valid"));
        assert_eq!(status(&report, "caller_visible_denial"), "refuted");
        assert!(report
            .findings
            .iter()
            .any(|f| f.id == "caller_visible_outcome_contradiction"));
    }

    #[test]
    fn incomplete_cells_carry_no_source_class() {
        let report = profile_report(&[ev(decision_payload("deny"), 0)]);
        let claims = serde_json::to_value(report.claims.as_ref().unwrap()).unwrap();
        assert!(claims["caller_visible_denial"]
            .get("source_class")
            .is_none());
        assert_eq!(
            claims["policy_decision_recorded"]["source_class"],
            json!("producer_reported")
        );
    }

    #[test]
    fn two_decisions_are_invalid_never_paired() {
        let report = profile_report(&[
            ev(decision_payload("deny"), 0),
            ev(decision_payload("allow"), 1),
        ]);
        assert_eq!(report.verdict, Some("invalid"));
        assert!(report.claims.is_none());
        assert!(report
            .findings
            .iter()
            .any(|f| f.id == "decision_cardinality"));
    }

    #[test]
    fn unknown_in_namespace_schema_fails_closed() {
        let report = profile_report(&[
            ev(decision_payload("deny"), 0),
            ev(
                json!({"schema": "assay.enforcement_decision.v1", "decision": "deny"}),
                1,
            ),
        ]);
        assert_eq!(report.verdict, Some("invalid"));
        let unknown = report
            .findings
            .iter()
            .find(|f| f.id == "unknown_profile_schema")
            .expect("unknown_profile_schema");
        assert_eq!(
            unknown.observed_schema.as_deref(),
            Some("assay.enforcement_decision.v1")
        );
    }

    #[test]
    fn fail_closed_divergence_is_invalid() {
        let mut dec = decision_payload("deny");
        dec["fail_closed"] = json!(false);
        let report = profile_report(&[ev(dec, 0)]);
        assert_eq!(report.verdict, Some("invalid"));
        assert!(report
            .findings
            .iter()
            .any(|f| f.id == "fail_closed_derivation"));
    }

    #[test]
    fn missing_producer_non_claim_is_invalid() {
        let mut dec = decision_payload("deny");
        dec["non_claims"] = json!(DECISION_PRODUCER_NON_CLAIMS[..4].to_vec());
        let report = profile_report(&[ev(dec, 0)]);
        assert_eq!(report.verdict, Some("invalid"));
        assert!(report
            .findings
            .iter()
            .any(|f| f.id == "decision_non_claims"));
    }

    #[test]
    fn marker_binding_mismatch_is_invalid() {
        let mut obs = observation_payload();
        obs["call"]["target_digest"] =
            json!("sha256:0000000000000000000000000000000000000000000000000000000000000000");
        let report = profile_report(&[ev(decision_payload("deny"), 0), ev(obs, 1)]);
        assert_eq!(report.verdict, Some("invalid"));
        assert!(report
            .findings
            .iter()
            .any(|f| f.id == "observation_binding"));
    }

    #[test]
    fn marker_with_null_digest_is_unbindable() {
        let mut obs = observation_payload();
        obs["call"]["target_digest"] = Value::Null;
        let report = profile_report(&[ev(decision_payload("deny"), 0), ev(obs, 1)]);
        assert_eq!(report.verdict, Some("invalid"));
        assert!(report
            .findings
            .iter()
            .any(|f| f.id == "marker_digest_unbindable"));
    }

    #[test]
    fn establish_contradiction_is_a_finding_not_a_cell_change() {
        let report = profile_report(&[
            ev(decision_payload("allow"), 0),
            ev(establish_payload("immediate_deny"), 1),
        ]);
        assert_eq!(report.verdict, Some("valid"));
        assert_eq!(status(&report, "caller_visible_denial"), "incomplete");
        assert!(report
            .findings
            .iter()
            .any(|f| f.id == "establish_journey_contradiction"));
    }

    #[test]
    fn establish_attempted_divergence_is_invalid() {
        let mut est = establish_payload("no_establish_needed");
        est["run_outcome"] = json!("not_performed");
        let report = profile_report(&[ev(decision_payload("allow"), 0), ev(est, 1)]);
        assert_eq!(report.verdict, Some("invalid"));
        assert!(report
            .findings
            .iter()
            .any(|f| f.id == "establish_attempted_derivation"));
    }

    #[test]
    fn report_always_carries_the_four_fixed_non_claims() {
        let report = profile_report(&[ev(decision_payload("deny"), 0)]);
        let value = serde_json::to_value(&report).unwrap();
        assert_eq!(value["non_claims"], json!(REPORT_NON_CLAIMS.to_vec()));
        assert_eq!(value["schema"], json!(REPORT_SCHEMA));
        assert_eq!(value["profile"], json!(PROFILE_ID));
    }

    #[test]
    fn invalid_report_omits_claims_member_entirely() {
        let report = profile_report(&[]);
        assert_eq!(report.verdict, Some("invalid"));
        let value = serde_json::to_value(&report).unwrap();
        assert!(value.get("claims").is_none());
    }

    #[test]
    fn stage1_diagnosis_consumes_reachable_limit_and_path_codes() {
        use assay_evidence::{ErrorClass, ErrorCode, VerifyError};

        for code in [
            ErrorCode::LimitBundleBytes,
            ErrorCode::LimitDecodeBytes,
            ErrorCode::LimitFileSize,
            ErrorCode::LimitLineBytes,
            ErrorCode::LimitTotalEvents,
            ErrorCode::LimitPathLength,
            ErrorCode::LimitJsonDepth,
        ] {
            let err = anyhow::Error::new(VerifyError::new(ErrorClass::Limits, code, "ceiling"));
            let value = serde_json::to_value(integrity_fail_report(
                &err,
                Path::new("synthetic.bundle"),
                ProfileVersion::V0,
                ProfileSelection::Default,
            ))
            .unwrap();
            assert_eq!(value["reason_code"], "E_EVIDENCE_LIMIT_EXCEEDED", "{code}");
            assert_ne!(value["reason_code"], "E_EVIDENCE_INTEGRITY", "{code}");
            assert_ne!(value["reason_code"], "E_EVIDENCE_CONTRACT", "{code}");
            assert_ne!(value["reason_code"], "E_EVIDENCE_UNREADABLE", "{code}");
            assert!(
                !value["next_step"].as_str().unwrap_or("").is_empty(),
                "{code}: emitted next_step must be non-empty"
            );
            assert!(value.get("claims").is_none(), "{code}");
        }

        let path_err = anyhow::Error::new(VerifyError::new(
            ErrorClass::Security,
            ErrorCode::SecurityPathTraversal,
            "path",
        ));
        let path_value = serde_json::to_value(integrity_fail_report(
            &path_err,
            Path::new("synthetic.bundle"),
            ProfileVersion::V0,
            ProfileSelection::Default,
        ))
        .unwrap();
        assert_eq!(path_value["reason_code"], "E_EVIDENCE_PATH_REJECTED");
        assert_ne!(path_value["reason_code"], "E_EVIDENCE_LIMIT_EXCEEDED");
        assert!(
            !path_value["next_step"].as_str().unwrap_or("").is_empty(),
            "path next_step must be non-empty"
        );
    }

    fn assert_unreadable_stage1(report: &Report, bundle: &Path) {
        let value = serde_json::to_value(report).unwrap();
        let path = bundle.to_str().expect("utf-8 test path");
        assert_eq!(value["schema"], REPORT_SCHEMA);
        assert_ne!(value["schema"], "assay.run_summary.v1");
        assert_eq!(value["profile"], PROFILE_ID);
        assert_eq!(value["bundle_integrity"], "fail");
        assert!(value.get("verdict").is_none());
        assert!(value.get("claims").is_none());
        assert_eq!(value["reason_code"], "E_EVIDENCE_UNREADABLE");
        let detail = value["findings"][0]["detail"]
            .as_str()
            .expect("bundle_integrity finding detail");
        assert!(
            detail.contains(path),
            "findings.detail must contain the caller bundle argv: {detail}"
        );
        // Caller argv is safe to republish shell-free, unlike discovered host paths.
        let expected_next = ReasonCode::EEvidenceUnreadable.next_step(Some(path));
        assert_eq!(value["next_step"].as_str(), Some(expected_next.as_str()));
        assert!(
            expected_next.starts_with("Run argv:"),
            "unreadable next_step must be JSON argv, not a shell string"
        );
        assert!(
            expected_next.contains(r#""--""#),
            "unreadable next_step must keep the positional separator: {expected_next}"
        );
        assert_eq!(value["non_claims"], json!(REPORT_NON_CLAIMS.to_vec()));
    }

    #[test]
    fn missing_bundle_path_is_unreadable_stage1_report() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let missing = tmp.path().join("missing.bundle.tar.gz");
        assert!(!missing.exists(), "the missing-bundle child must not exist");
        assert_unreadable_stage1(&verify_bundle_report(&missing), &missing);
    }

    #[test]
    fn directory_bundle_path_is_unreadable_stage1_report() {
        let tmp = tempfile::tempdir().expect("tempdir");
        assert_unreadable_stage1(&verify_bundle_report(tmp.path()), tmp.path());
    }

    #[test]
    fn post_open_unreadable_bundle_detail_contains_caller_argv() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let garbage = tmp.path().join("not-a-bundle.tar.gz");
        std::fs::write(&garbage, b"not a gzip archive").expect("write garbage bundle");
        assert_unreadable_stage1(&verify_bundle_report(&garbage), &garbage);
    }

    #[test]
    fn success_report_omits_diagnosis_fields() {
        let report = profile_report(&[ev(decision_payload("deny"), 0)]);
        let value = serde_json::to_value(&report).unwrap();
        assert!(value.get("reason_code").is_none());
        assert!(value.get("next_step").is_none());
    }

    #[test]
    fn default_profile_without_observation_is_v0() {
        let report = profile_report(&[ev(decision_payload("deny"), 0)]);
        assert_eq!(report.schema, REPORT_SCHEMA);
        assert_eq!(report.profile, PROFILE_ID);
        assert_eq!(status(&report, "caller_visible_denial"), "incomplete");
    }

    #[test]
    fn explicit_v1_without_observation_reports_v1() {
        let report = profile_report_for(&[ev(decision_payload("deny"), 0)], ProfileVersion::V1);
        assert_eq!(report.schema, REPORT_SCHEMA);
        assert_eq!(report.profile, PROFILE_ID_V1);
        assert_eq!(status(&report, "caller_visible_denial"), "incomplete");
    }

    #[test]
    fn v1_bound_marker_confirms_caller_visible_denial() {
        let report = profile_report_for(
            &[
                ev(decision_payload("deny"), 0),
                ev(observation_payload_v1(), 1),
            ],
            ProfileVersion::V1,
        );
        assert_eq!(report.verdict, Some("valid"));
        assert_eq!(report.profile, PROFILE_ID_V1);
        assert_eq!(status(&report, "caller_visible_denial"), "confirmed");
    }

    #[test]
    fn v1_observation_under_default_v0_fails_closed() {
        let report = profile_report(&[
            ev(decision_payload("deny"), 0),
            ev(observation_payload_v1(), 1),
        ]);
        assert_eq!(report.profile, PROFILE_ID);
        assert_eq!(report.verdict, Some("invalid"));
        let unknown = report
            .findings
            .iter()
            .find(|f| f.id == "unknown_profile_schema")
            .expect("unknown_profile_schema");
        assert_eq!(
            unknown.observed_schema.as_deref(),
            Some("assay.denied_call_observation.v1")
        );
    }

    #[test]
    fn v0_observation_under_v1_fails_closed() {
        let report = profile_report_for(
            &[
                ev(decision_payload("deny"), 0),
                ev(observation_payload(), 1),
            ],
            ProfileVersion::V1,
        );
        assert_eq!(report.profile, PROFILE_ID_V1);
        assert_eq!(report.verdict, Some("invalid"));
        assert!(report
            .findings
            .iter()
            .any(|f| f.id == "unknown_profile_schema"));
    }

    #[test]
    fn mixed_v0_and_v1_observations_fail_closed() {
        let report = profile_report(&[
            ev(decision_payload("deny"), 0),
            ev(observation_payload(), 1),
            ev(observation_payload_v1(), 2),
        ]);
        assert_eq!(report.verdict, Some("invalid"));
        assert!(report
            .findings
            .iter()
            .any(|f| f.id == "unknown_profile_schema"));
    }

    #[test]
    fn v1_schema_with_legacy_code_is_inert_under_v1() {
        let mut obs = observation_payload_v1();
        obs["caller_visible_error"]["code"] = json!(-32042);
        let report = profile_report_for(
            &[ev(decision_payload("deny"), 0), ev(obs, 1)],
            ProfileVersion::V1,
        );
        assert_eq!(report.verdict, Some("valid"));
        assert_eq!(status(&report, "caller_visible_denial"), "incomplete");
    }
}
