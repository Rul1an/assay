use anyhow::{Context, Result};
use clap::{ArgGroup, Args, ValueEnum};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Args, Clone)]
#[command(group(
    ArgGroup::new("binding_input")
        .required(true)
        .args(["attestation", "request_envelope"])
))]
pub struct McpExecutionRecordArgs {
    /// SEP-2787 attestation JSON fixture
    #[arg(long)]
    pub attestation: Option<PathBuf>,

    /// Observed tools/call request envelope JSON fixture for no-attestation fallback
    #[arg(long)]
    pub request_envelope: Option<PathBuf>,

    /// Server-side decision record JSON fixture
    #[arg(long)]
    pub decision: PathBuf,

    /// Optional server-side outcome record JSON fixture
    #[arg(long)]
    pub outcome: Option<PathBuf>,

    /// For the no-attestation fallback, how the request-envelope binding digest is computed.
    /// `whole-envelope` (default) is the legacy compatibility mode: it digests the full JCS envelope.
    /// `named` is the named fallback projection mode: it digests only the `tools/call` params plus the
    /// `_meta.authorization_binding` block, so transport-local or observation-local `_meta` fields a
    /// gateway/provider can legitimately add or strip do not change the digest. Named mode is
    /// allowlist + fail-closed: if the binding block cannot be resolved the fallback case is
    /// non-conformant rather than silently hashing the whole envelope. Ignored for the SEP-2787 path.
    #[arg(long, value_enum, default_value_t = FallbackProjection::WholeEnvelope)]
    pub fallback_projection: FallbackProjection,

    /// Output format
    #[arg(long, value_enum, default_value_t = McpExecutionRecordFormat::Table)]
    pub format: McpExecutionRecordFormat,
}

/// Self-describing id of the named fallback projection. A rename or rule change is an explicit
/// version bump (it tracks the in-progress SEP-2828 fallback-binding discussion), never a silent
/// reinterpretation. The binding block is read at `_meta.authorization_binding`.
const FALLBACK_PROJECTION_V0: &str = "assay.fallback_projection.v0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum FallbackProjection {
    /// Legacy compatibility mode: digest the full JCS-canonical request envelope.
    WholeEnvelope,
    /// Named fallback projection mode: digest only the `tools/call` params plus the
    /// `_meta.authorization_binding` block (allowlist + fail-closed).
    Named,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum McpExecutionRecordFormat {
    Json,
    Table,
}

#[derive(Debug, Serialize)]
struct PairingReport {
    schema: &'static str,
    ok: bool,
    canonicalization: &'static str,
    verification_scope: VerificationScope,
    binding: BindingReport,
    attestation: Option<AttestationReport>,
    decision: DecisionReport,
    outcome: Option<OutcomeReport>,
    checks: Vec<CheckReport>,
    claims_not_made: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
struct VerificationScope {
    role: &'static str,
    note: &'static str,
}

#[derive(Debug, Serialize)]
struct AttestationReport {
    digest: String,
    nonce: Option<String>,
}

#[derive(Debug, Serialize)]
struct BindingReport {
    mode: &'static str,
    digest: String,
    digest_source: &'static str,
    /// Self-describing projection id for the named fallback; `None` for whole-envelope / attestation.
    projection: Option<&'static str>,
    nonce: Option<String>,
    nonce_source: &'static str,
}

#[derive(Debug, Serialize)]
struct DecisionReport {
    decision: Option<String>,
    decided_at: Option<String>,
    backlink: BackLinkReport,
    signature_present: bool,
}

#[derive(Debug, Serialize)]
struct OutcomeReport {
    status: Option<String>,
    completed_at: Option<String>,
    decision_digest: Option<String>,
    backlink: BackLinkReport,
    signature_present: bool,
}

#[derive(Debug, Serialize)]
struct BackLinkReport {
    attestation_digest: Option<String>,
    attestation_nonce: Option<String>,
}

#[derive(Debug, Serialize)]
struct CheckReport {
    id: &'static str,
    ok: bool,
    detail: String,
}

pub fn cmd_verify_mcp_records(args: McpExecutionRecordArgs) -> Result<i32> {
    let binding_input = match (&args.attestation, &args.request_envelope) {
        (Some(attestation), None) => BindingInput::Attestation(read_json(attestation)?),
        (None, Some(request_envelope)) => {
            BindingInput::RequestEnvelope(read_json(request_envelope)?)
        }
        _ => anyhow::bail!("exactly one of --attestation or --request-envelope is required"),
    };
    let decision = read_json(&args.decision)?;
    let outcome = args.outcome.as_ref().map(read_json).transpose()?;

    let report = build_report(
        &binding_input,
        &decision,
        outcome.as_ref(),
        args.fallback_projection,
    )?;
    match args.format {
        McpExecutionRecordFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        McpExecutionRecordFormat::Table => print_table_report(&report),
    }

    Ok(if report.ok { 0 } else { 2 })
}

fn read_json(path: &PathBuf) -> Result<Value> {
    let body =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&body).with_context(|| format!("failed to parse {}", path.display()))
}

enum BindingInput {
    Attestation(Value),
    RequestEnvelope(Value),
}

struct BindingExpectation {
    mode: &'static str,
    digest: String,
    digest_source: &'static str,
    projection: Option<&'static str>,
    /// `Some(false)` when named projection was requested but the binding block could not be resolved
    /// (fail-closed); `None` when not applicable (whole-envelope / attestation).
    binding_block_present: Option<bool>,
    /// Stable reason code for the named-projection fail-closed case (None when present / N/A).
    named_fail_code: Option<&'static str>,
    nonce: Option<String>,
    nonce_source: &'static str,
}

fn build_report(
    binding_input: &BindingInput,
    decision: &Value,
    outcome: Option<&Value>,
    fallback_projection: FallbackProjection,
) -> Result<PairingReport> {
    let decision_digest = jcs_digest(decision).context("failed to digest decision")?;
    let decision_backlink = backlink_report(decision)?;
    let outcome_backlink = outcome.map(backlink_report).transpose()?;
    let expectation = binding_expectation(binding_input, &decision_backlink, fallback_projection)?;

    let mut checks = Vec::new();
    // Fail-closed: named projection requested but the binding block could not be resolved is
    // non-conformant, never a silent fall-back to hashing the whole envelope. The check id is the
    // stable reason code (invalid `_meta` vs missing `authorization_binding`).
    match (
        expectation.binding_block_present,
        expectation.named_fail_code,
    ) {
        (Some(true), _) => checks.push(CheckReport {
            id: "fallback_projection_binding_present",
            ok: true,
            detail: "named fallback projection binding block present".to_string(),
        }),
        (Some(false), Some(code)) => checks.push(CheckReport {
            id: code,
            ok: false,
            detail: "named fallback projection requested but the binding block could not be \
                     resolved; failing closed instead of hashing the whole envelope"
                .to_string(),
        }),
        _ => {}
    }
    push_decision_binding_checks(&mut checks, &decision_backlink, &expectation);
    checks.push(check_enum(
        "decision_enum",
        decision_value(decision).as_deref(),
        &["allow", "block", "escalate"],
    ));

    if let Some(outcome_backlink) = &outcome_backlink {
        push_outcome_binding_checks(&mut checks, outcome_backlink, &expectation);
        checks.push(check_eq(
            "decision_outcome_backlink_match",
            backlink_pair_key(outcome_backlink).as_deref(),
            backlink_pair_key(&decision_backlink).as_deref(),
            "decision and outcome describe the same call instance",
        ));
        // SEP-2828 Check B digests the full signed decision record.
        checks.push(check_eq(
            "outcome_decision_digest_match",
            outcome.and_then(outcome_decision_digest).as_deref(),
            Some(decision_digest.as_str()),
            "outcomeDerived.decisionDigest matches the signed decision record digest",
        ));
        checks.push(check_enum(
            "outcome_status_enum",
            outcome.and_then(outcome_status).as_deref(),
            &["executed", "refused", "errored"],
        ));
    } else {
        checks.push(CheckReport {
            id: "outcome_absent",
            ok: true,
            detail: "no outcome record supplied; report is decision-only".to_string(),
        });
    }

    let decision_report = DecisionReport {
        decision: decision_value(decision),
        decided_at: string_at(decision, &["decisionDerived", "decidedAt"]),
        backlink: decision_backlink,
        signature_present: decision.get("signature").and_then(Value::as_str).is_some(),
    };
    let outcome_report = match (outcome, outcome_backlink) {
        (Some(outcome), Some(backlink)) => Some(OutcomeReport {
            status: outcome_status(outcome),
            completed_at: string_at(outcome, &["outcomeDerived", "completedAt"]),
            decision_digest: outcome_decision_digest(outcome),
            backlink,
            signature_present: outcome.get("signature").and_then(Value::as_str).is_some(),
        }),
        _ => None,
    };

    let ok = checks.iter().all(|check| check.ok);
    Ok(PairingReport {
        schema: "assay.mcp.execution-record-pairing.report.v0",
        ok,
        canonicalization: "jcs/rfc8785",
        verification_scope: VerificationScope {
            role: "independent-consumer",
            note: "Assay verifies fixture pairing and digest commitments only; it does not emit records or act as a proxy.",
        },
        binding: BindingReport {
            mode: expectation.mode,
            digest: expectation.digest.clone(),
            digest_source: expectation.digest_source,
            projection: expectation.projection,
            nonce: expectation.nonce.clone(),
            nonce_source: expectation.nonce_source,
        },
        attestation: attestation_report(binding_input, &expectation),
        decision: decision_report,
        outcome: outcome_report,
        checks,
        claims_not_made: claims_not_made(&expectation),
    })
}

fn claims_not_made(expectation: &BindingExpectation) -> Vec<&'static str> {
    let mut claims = vec![
        "signature_verification",
        "issuer_key_trust",
        "policy_correctness",
        "runtime_side_effect_truth",
        "payload_or_result_disclosure",
    ];
    if expectation.mode == "request_envelope" {
        claims.push("fallback_server_observation_truth");
        claims.push("fallback_nonce_freshness_or_uniqueness");
    }
    claims
}

fn binding_expectation(
    binding_input: &BindingInput,
    decision_backlink: &BackLinkReport,
    fallback_projection: FallbackProjection,
) -> Result<BindingExpectation> {
    match binding_input {
        BindingInput::Attestation(attestation) => Ok(BindingExpectation {
            mode: "sep2787_attestation",
            digest: jcs_digest(attestation).context("failed to digest attestation")?,
            digest_source: "sep2787_attestation_jcs",
            projection: None,
            binding_block_present: None,
            named_fail_code: None,
            nonce: string_at(attestation, &["issuerAsserted", "nonce"]),
            nonce_source: "issuerAsserted.nonce",
        }),
        BindingInput::RequestEnvelope(request_envelope) => match fallback_projection {
            FallbackProjection::WholeEnvelope => Ok(BindingExpectation {
                mode: "request_envelope",
                digest: jcs_digest(request_envelope)
                    .context("failed to digest request envelope")?,
                digest_source: "request_envelope_jcs",
                projection: None,
                binding_block_present: None,
                named_fail_code: None,
                nonce: decision_backlink.attestation_nonce.clone(),
                nonce_source: "record_backlink_consistency",
            }),
            FallbackProjection::Named => {
                // Allowlist: the preimage is exactly the named params plus the whole named binding
                // block, everything else under _meta is excluded by construction. Fail-closed with a
                // stable reason when the binding cannot be resolved — never hash the whole envelope.
                // `_meta` absent or not an object -> invalid_meta; object but no binding -> missing.
                let meta = request_envelope.get("_meta");
                let (named_fail_code, binding): (Option<&'static str>, Option<&Value>) = match meta
                {
                    None => (Some("fallback_projection_invalid_meta"), None),
                    Some(m) if !m.is_object() => (Some("fallback_projection_invalid_meta"), None),
                    Some(m) => match m.get("authorization_binding") {
                        Some(b) => (None, Some(b)),
                        None => (
                            Some("fallback_projection_missing_authorization_binding"),
                            None,
                        ),
                    },
                };
                let digest = match binding {
                    Some(binding) => {
                        // The whole authorization_binding object is bound (bind-all); any field
                        // inside it is part of the preimage. The projection id is bound too.
                        let projected = serde_json::json!({
                            "projection": FALLBACK_PROJECTION_V0,
                            "params": request_envelope.get("params").unwrap_or(&Value::Null),
                            "binding": binding,
                        });
                        jcs_digest(&projected)
                            .context("failed to digest named fallback projection")?
                    }
                    None => "sha256:unresolved-binding-block".to_string(),
                };
                Ok(BindingExpectation {
                    mode: "request_envelope",
                    digest,
                    digest_source: "request_envelope_named_projection_jcs",
                    projection: Some(FALLBACK_PROJECTION_V0),
                    binding_block_present: Some(binding.is_some()),
                    named_fail_code,
                    nonce: decision_backlink.attestation_nonce.clone(),
                    nonce_source: "record_backlink_consistency",
                })
            }
        },
    }
}

fn attestation_report(
    binding_input: &BindingInput,
    expectation: &BindingExpectation,
) -> Option<AttestationReport> {
    match binding_input {
        BindingInput::Attestation(_) => Some(AttestationReport {
            digest: expectation.digest.clone(),
            nonce: expectation.nonce.clone(),
        }),
        BindingInput::RequestEnvelope(_) => None,
    }
}

fn push_decision_binding_checks(
    checks: &mut Vec<CheckReport>,
    decision_backlink: &BackLinkReport,
    expectation: &BindingExpectation,
) {
    match expectation.mode {
        "sep2787_attestation" => {
            checks.push(check_eq(
                "decision_attestation_digest_match",
                decision_backlink.attestation_digest.as_deref(),
                Some(expectation.digest.as_str()),
                "decision backLink.attestationDigest matches SEP-2787 JCS digest",
            ));
            checks.push(check_eq(
                "decision_attestation_nonce_match",
                decision_backlink.attestation_nonce.as_deref(),
                expectation.nonce.as_deref(),
                "decision backLink.attestationNonce matches issuerAsserted.nonce",
            ));
        }
        "request_envelope" => {
            checks.push(check_eq(
                "decision_request_envelope_digest_match",
                decision_backlink.attestation_digest.as_deref(),
                Some(expectation.digest.as_str()),
                "decision backLink.attestationDigest matches request-envelope JCS digest",
            ));
            checks.push(check_present(
                "decision_request_envelope_nonce_present",
                decision_backlink.attestation_nonce.as_deref(),
                "decision backLink.attestationNonce is present for fallback binding",
            ));
        }
        _ => unreachable!("unknown binding mode"),
    }
}

fn push_outcome_binding_checks(
    checks: &mut Vec<CheckReport>,
    outcome_backlink: &BackLinkReport,
    expectation: &BindingExpectation,
) {
    match expectation.mode {
        "sep2787_attestation" => {
            checks.push(check_eq(
                "outcome_attestation_digest_match",
                outcome_backlink.attestation_digest.as_deref(),
                Some(expectation.digest.as_str()),
                "outcome backLink.attestationDigest matches SEP-2787 JCS digest",
            ));
            checks.push(check_eq(
                "outcome_attestation_nonce_match",
                outcome_backlink.attestation_nonce.as_deref(),
                expectation.nonce.as_deref(),
                "outcome backLink.attestationNonce matches issuerAsserted.nonce",
            ));
        }
        "request_envelope" => {
            checks.push(check_eq(
                "outcome_request_envelope_digest_match",
                outcome_backlink.attestation_digest.as_deref(),
                Some(expectation.digest.as_str()),
                "outcome backLink.attestationDigest matches request-envelope JCS digest",
            ));
        }
        _ => unreachable!("unknown binding mode"),
    }
}

fn jcs_digest(value: &Value) -> Result<String> {
    let canonical = assay_core::mcp::jcs::to_vec(value)?;
    let hash = Sha256::digest(&canonical);
    Ok(format!("sha256:{}", hex::encode(hash)))
}

fn backlink_report(record: &Value) -> Result<BackLinkReport> {
    let backlink = record
        .get("backLink")
        .or_else(|| record.get("back_link"))
        .ok_or_else(|| anyhow::anyhow!("record missing backLink"))?;
    Ok(BackLinkReport {
        attestation_digest: string_at(backlink, &["attestationDigest"])
            .or_else(|| string_at(backlink, &["attestation_digest"])),
        attestation_nonce: string_at(backlink, &["attestationNonce"])
            .or_else(|| string_at(backlink, &["attestation_nonce"])),
    })
}

fn decision_value(record: &Value) -> Option<String> {
    string_at(record, &["decisionDerived", "decision"])
        .or_else(|| string_at(record, &["decision_derived", "decision"]))
}

fn outcome_status(record: &Value) -> Option<String> {
    string_at(record, &["outcomeDerived", "status"])
        .or_else(|| string_at(record, &["outcome_derived", "status"]))
}

fn outcome_decision_digest(record: &Value) -> Option<String> {
    string_at(record, &["outcomeDerived", "decisionDigest"])
        .or_else(|| string_at(record, &["outcome_derived", "decision_digest"]))
}

fn backlink_pair_key(backlink: &BackLinkReport) -> Option<String> {
    Some(format!(
        "attestationDigest={};attestationNonce={}",
        backlink.attestation_digest.as_deref()?,
        backlink.attestation_nonce.as_deref()?
    ))
}

fn string_at(value: &Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for segment in path {
        current = current.get(*segment)?;
    }
    current.as_str().map(ToOwned::to_owned)
}

fn check_eq(
    id: &'static str,
    left: Option<&str>,
    right: Option<&str>,
    description: &str,
) -> CheckReport {
    let ok = left.is_some() && right.is_some() && left == right;
    let detail = match (left, right) {
        (Some(left), Some(right)) if left == right => description.to_string(),
        (Some(left), Some(right)) => format!("mismatch: got {left}, expected {right}"),
        (None, _) => "missing observed value".to_string(),
        (_, None) => "missing expected value".to_string(),
    };
    CheckReport { id, ok, detail }
}

fn check_enum(id: &'static str, value: Option<&str>, allowed: &[&str]) -> CheckReport {
    match value {
        Some(value) if allowed.contains(&value) => CheckReport {
            id,
            ok: true,
            detail: format!("{value} is allowed"),
        },
        Some(value) => CheckReport {
            id,
            ok: false,
            detail: format!("{value} is not one of {}", allowed.join(", ")),
        },
        None => CheckReport {
            id,
            ok: false,
            detail: "missing value".to_string(),
        },
    }
}

fn check_present(id: &'static str, value: Option<&str>, description: &str) -> CheckReport {
    CheckReport {
        id,
        ok: value.is_some(),
        detail: if value.is_some() {
            description.to_string()
        } else {
            "missing value".to_string()
        },
    }
}

fn print_table_report(report: &PairingReport) {
    println!("MCP Execution Record Pairing Report");
    println!("===================================");
    println!("OK:               {}", if report.ok { "yes" } else { "no" });
    println!("Role:             {}", report.verification_scope.role);
    println!("Binding:          {}", report.binding.mode);
    println!("Binding digest:   {}", report.binding.digest);
    println!(
        "Binding nonce:    {}",
        report.binding.nonce.as_deref().unwrap_or("-")
    );
    println!(
        "Decision:         {}",
        report.decision.decision.as_deref().unwrap_or("-")
    );
    if let Some(outcome) = &report.outcome {
        println!(
            "Outcome:          {}",
            outcome.status.as_deref().unwrap_or("-")
        );
    } else {
        println!("Outcome:          -");
    }
    println!();
    for check in &report.checks {
        println!(
            "{:<36} {:<4} {}",
            check.id,
            if check.ok { "ok" } else { "fail" },
            check.detail
        );
    }
    println!();
    println!("Claims not made: {}", report.claims_not_made.join(", "));
}
