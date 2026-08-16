//! EXPERIMENTAL: verify an evidence bundle against the privileged-mcp-action/v0 open profile.
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

use crate::evidence_verify_reason::reason_code_for_verify_error;
use crate::exit_codes::{self, ReasonCode};
use anyhow::{Context, Result};
use assay_evidence::bundle::BundleReader;
use assay_evidence::types::EvidenceEvent;
use clap::{Args, ValueEnum};
use serde::Serialize;
use serde_json::Value;
use std::fs::File;
use std::path::{Path, PathBuf};

const REPORT_SCHEMA: &str = "assay.privileged_mcp_action.verify.report.v0";
const PROFILE_ID: &str = "privileged-mcp-action/v0";

const DECISION_SCHEMA: &str = "assay.enforcement_decision.v0";
const OBSERVATION_SCHEMA: &str = "assay.denied_call_observation.v0";
const ESTABLISH_SCHEMA: &str = "assay.manifest_establish.v0";
/// Profile namespaces: any payload schema with one of these prefixes that is not the exact `.v0`
/// string is an unrecognized profile schema and fails closed.
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

/// Marker triple values (stage 3).
const MARKER_ERROR_CODE: i64 = -32042;
const MARKER_ORIGIN: &str = "assay-proxy";

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
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum VerifyFormat {
    Json,
    Table,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub schema: &'static str,
    pub profile: &'static str,
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
    /// Context-invariant prose remediation. Absent on success.
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
}

pub fn cmd_verify_privileged_mcp_action(args: VerifyPrivilegedMcpActionArgs) -> Result<i32> {
    let report = verify_bundle_report(&args.bundle)?;
    match args.format {
        VerifyFormat::Json => println!("{}", serde_json::to_string_pretty(&report)?),
        VerifyFormat::Table => print_table(&report),
    }
    let ok = report.bundle_integrity == "pass" && report.verdict == Some("valid");
    Ok(if ok { exit_codes::OK } else { 2 })
}

/// Stage 1 + stages 2-4 over one bundle path. A file that cannot be opened at all is a usage error;
/// a bundle that opens but fails verification is an integrity-fail report, never an error.
pub fn verify_bundle_report(bundle: &Path) -> Result<Report> {
    let file = File::open(bundle)
        .with_context(|| format!("failed to open bundle {}", bundle.display()))?;
    // Stage 1: BundleReader::open runs the full shipped bundle verification
    // (verify_bundle_with_limits with default limits) before exposing any event.
    let reader = match BundleReader::open(file) {
        Ok(reader) => reader,
        Err(err) => return Ok(integrity_fail_report(&err)),
    };
    let events = match reader.events_vec() {
        Ok(events) => events,
        Err(err) => return Ok(integrity_fail_report(&err)),
    };
    Ok(profile_report(&events))
}

fn integrity_fail_report(err: &anyhow::Error) -> Report {
    let diagnosis = err
        .chain()
        .find_map(|cause| cause.downcast_ref::<assay_evidence::VerifyError>())
        .and_then(reason_code_for_verify_error);
    Report {
        schema: REPORT_SCHEMA,
        profile: PROFILE_ID,
        bundle_integrity: "fail",
        verdict: None,
        claims: None,
        findings: vec![Finding {
            id: "bundle_integrity".to_string(),
            detail: format!("{err:#}"),
        }],
        non_claims: REPORT_NON_CLAIMS,
        reason_code: diagnosis.map(|reason| reason.as_str()),
        next_step: diagnosis.map(|reason| reason.next_step(None)),
    }
}

/// Stages 2-4 over the events of a bundle that already passed stage 1.
pub fn profile_report(events: &[EvidenceEvent]) -> Report {
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
                    OBSERVATION_SCHEMA => observations.push(&ev.payload),
                    ESTABLISH_SCHEMA => establishes.push(&ev.payload),
                    other => violations.push(finding(
                        "unknown_profile_schema",
                        format!(
                            "payload schema {other:?} is inside the profile namespace but is not a recognized v0 record; unknown fails closed"
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
                "at most one assay.denied_call_observation.v0 payload is allowed, found {}",
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
    // if the triple holds (schema exact + error code + origin); a marker must bind to the decision.
    let marker = observation.filter(|obs| {
        obs.get("caller_visible_error")
            .map(|e| {
                e.get("code").and_then(Value::as_i64) == Some(MARKER_ERROR_CODE)
                    && e.get("origin").and_then(Value::as_str) == Some(MARKER_ORIGIN)
            })
            .unwrap_or(false)
    });
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
        return Report {
            schema: REPORT_SCHEMA,
            profile: PROFILE_ID,
            bundle_integrity: "pass",
            verdict: Some("invalid"),
            claims: None,
            findings: violations,
            non_claims: REPORT_NON_CLAIMS,
            reason_code: Some(ReasonCode::EEvidenceProfileInvalid.as_str()),
            next_step: Some(ReasonCode::EEvidenceProfileInvalid.next_step(None)),
        };
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

    Report {
        schema: REPORT_SCHEMA,
        profile: PROFILE_ID,
        bundle_integrity: "pass",
        verdict: Some("valid"),
        claims: Some(Claims {
            policy_decision_recorded: ClaimCell::confirmed(),
            caller_visible_denial,
            upstream_delivery: ClaimCell::incomplete(),
            external_side_effect: ClaimCell::incomplete(),
        }),
        findings: notes,
        non_claims: REPORT_NON_CLAIMS,
        reason_code: None,
        next_step: None,
    }
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
    }
}

fn print_table(report: &Report) {
    println!("Privileged MCP Action Verification ({})", report.profile);
    println!("=====================================================");
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
        assert!(report
            .findings
            .iter()
            .any(|f| f.id == "unknown_profile_schema"));
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
            let value = serde_json::to_value(integrity_fail_report(&err)).unwrap();
            assert_eq!(value["reason_code"], "E_EVIDENCE_LIMIT_EXCEEDED", "{code}");
            assert_ne!(value["reason_code"], "E_EVIDENCE_INTEGRITY", "{code}");
            assert_ne!(value["reason_code"], "E_EVIDENCE_CONTRACT", "{code}");
            assert_ne!(value["reason_code"], "E_EVIDENCE_UNREADABLE", "{code}");
            assert!(value.get("claims").is_none(), "{code}");
        }

        let path_err = anyhow::Error::new(VerifyError::new(
            ErrorClass::Security,
            ErrorCode::SecurityPathTraversal,
            "path",
        ));
        let path_value = serde_json::to_value(integrity_fail_report(&path_err)).unwrap();
        assert_eq!(path_value["reason_code"], "E_EVIDENCE_PATH_REJECTED");
        assert_ne!(path_value["reason_code"], "E_EVIDENCE_LIMIT_EXCEEDED");

        let absolute = anyhow::Error::new(VerifyError::new(
            ErrorClass::Security,
            ErrorCode::SecurityAbsolutePath,
            "absolute",
        ));
        let absolute_value = serde_json::to_value(integrity_fail_report(&absolute)).unwrap();
        assert!(
            absolute_value.get("reason_code").is_none(),
            "SecurityAbsolutePath is enum-only; completeness does not require an emission"
        );
    }

    #[test]
    fn success_report_omits_diagnosis_fields() {
        let report = profile_report(&[ev(decision_payload("deny"), 0)]);
        let value = serde_json::to_value(&report).unwrap();
        assert!(value.get("reason_code").is_none());
        assert!(value.get("next_step").is_none());
    }
}
