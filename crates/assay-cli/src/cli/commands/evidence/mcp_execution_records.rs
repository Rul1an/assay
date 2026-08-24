use anyhow::{Context, Result};
use clap::{ArgGroup, Args, ValueEnum};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;

mod report;

use report::{
    print_table_report, AttestationReport, BackLinkReport, BindingReport, CheckReport,
    DecisionReport, OutcomeReport, PairingReport, ResultCommitmentReport, VerificationScope,
};

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
    /// `named` is the named fallback projection mode: it requires object-valued `params` and digests
    /// only those params plus the `_meta.authorization_binding` block, so transport-local or
    /// observation-local `_meta` fields a gateway/provider can legitimately add or strip do not
    /// change the digest. Named mode is allowlist + fail-closed: an incomplete preimage is
    /// non-conformant rather than silently hashing synthetic input. Ignored for the SEP-2787 path.
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
    digest: Option<String>,
    digest_source: &'static str,
    projection: Option<&'static str>,
    /// `Some(false)` when named projection was requested but its complete preimage could not be
    /// resolved (fail-closed); `None` when not applicable (whole-envelope / attestation).
    named_projection_ready: Option<bool>,
    /// Stable reason code for the named-projection fail-closed case (None when ready / N/A).
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
    let mut extra_claims: Vec<&'static str> = Vec::new();
    let mut result_commitment = None;
    // Fail-closed: named projection requested but the binding block could not be resolved is
    // non-conformant, never a silent fall-back to hashing the whole envelope. The check id is the
    // stable reason code (invalid `_meta` vs missing `authorization_binding`).
    match (
        expectation.named_projection_ready,
        expectation.named_fail_code,
    ) {
        (Some(true), _) => checks.push(CheckReport {
            id: "fallback_projection_binding_present",
            ok: true,
            detail: "named fallback projection preimage is complete".to_string(),
        }),
        (Some(false), Some(code)) => checks.push(CheckReport {
            id: code,
            ok: false,
            detail: "named fallback projection requested but its preimage could not be resolved; \
                     failing closed without publishing a binding digest"
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
        if let Some(outcome) = outcome {
            result_commitment =
                push_result_commitment_checks(&mut checks, &mut extra_claims, outcome);
        }
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
            result_commitment,
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
        claims_not_made: {
            let mut claims = claims_not_made(&expectation);
            claims.extend(extra_claims);
            claims
        },
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
    if matches!(
        expectation.named_fail_code,
        Some("fallback_projection_missing_params" | "fallback_projection_invalid_params")
    ) {
        claims.push("fallback_call_parameter_binding");
    }
    claims
}

/// SEP-2828 verification-algorithm step 5, split into the half a record-only consumer can settle
/// and the half it cannot.
///
/// An `ArgsProjection` commits `projectionDigest` as sha256 over the UTF-8 bytes of the
/// `projection` string, so a consumer holding nothing but the record recomputes it in full. An
/// `ArgsRef` addresses content this verifier never fetches. Neither shape says whether the
/// committed value is what the tool actually returned; that needs the runtime result, which a
/// record consumer does not have. The first half is now checked. The second is *declared* rather
/// than left to a reader's assumption, which is the point: an undeclared gap in coverage reads as
/// coverage.
fn push_result_commitment_checks(
    checks: &mut Vec<CheckReport>,
    extra_claims: &mut Vec<&'static str>,
    outcome: &Value,
) -> Option<ResultCommitmentReport> {
    let status = outcome_status(outcome);
    let commitment = outcome
        .get("outcomeDerived")
        .or_else(|| outcome.get("outcome_derived"))
        .and_then(|d| {
            d.get("resultCommitment")
                .or_else(|| d.get("result_commitment"))
        })
        // `Value::get` yields `Some(&Value::Null)` for a key present with an explicit null. A null
        // commits to nothing, so it is the same as no key at all; without this filter a refused
        // outcome would be failed for carrying a commitment it does not have.
        .filter(|commitment| !commitment.is_null());

    let Some(commitment) = commitment else {
        // Absence is only meaningful for `refused`, which by definition has no result.
        if status.as_deref() == Some("refused") {
            checks.push(CheckReport {
                id: "result_commitment_absent_for_refused",
                ok: true,
                detail: "refused outcome carries no resultCommitment".to_string(),
            });
        }
        return None;
    };

    if status.as_deref() == Some("refused") {
        checks.push(CheckReport {
            id: "result_commitment_absent_for_refused",
            ok: false,
            detail: "refused outcome carries a resultCommitment; a refusal has no result"
                .to_string(),
        });
    }

    // The committed value is never compared against a runtime result, in either shape.
    extra_claims.push("result_commitment_payload_binding");

    let projection = commitment.get("projection").and_then(Value::as_str);
    let projection_digest = string_at(commitment, &["projectionDigest"]);
    let reference = commitment.get("ref").and_then(Value::as_str);

    match (projection, reference) {
        // A commitment is one shape or the other. Carrying both leaves which one binds the result
        // undecided, so it is a producer defect rather than a licence to pick the first match.
        (Some(_), Some(_)) => {
            checks.push(CheckReport {
                id: "result_commitment_shape_recognized",
                ok: false,
                detail: "resultCommitment carries both `projection` and `ref`; a commitment is \
                         one shape or the other"
                    .to_string(),
            });
            Some(ResultCommitmentReport {
                kind: "ambiguous",
                projection_digest,
                ref_digest: string_at(commitment, &["digest"]),
                embedded_digest: None,
                recomputed_projection_digest: None,
            })
        }
        (Some(projection), None) => {
            let recomputed = format!(
                "sha256:{}",
                hex::encode(Sha256::digest(projection.as_bytes()))
            );
            checks.push(check_eq(
                "result_commitment_projection_digest_match",
                projection_digest.as_deref(),
                Some(recomputed.as_str()),
                "projectionDigest matches sha256 over the projection string bytes",
            ));
            // The RECOMMENDED hash-only-identity form embeds a digest of the withheld value. It is
            // surfaced so a reader can see what was committed to, and explicitly not checked.
            let embedded = serde_json::from_str::<Value>(projection)
                .ok()
                .and_then(|p| string_at(&p, &["digest"]));
            Some(ResultCommitmentReport {
                kind: "args_projection",
                projection_digest,
                ref_digest: None,
                embedded_digest: embedded,
                recomputed_projection_digest: Some(recomputed),
            })
        }
        (None, Some(_)) => {
            extra_claims.push("result_commitment_ref_not_dereferenced");
            // An ArgsRef's `digest` addresses referenced content. It is not a digest over a
            // projection string, so it does not go in `projection_digest`: a reader keying on that
            // field by name would compare two different quantities.
            Some(ResultCommitmentReport {
                kind: "args_ref",
                projection_digest: None,
                ref_digest: string_at(commitment, &["digest"]),
                embedded_digest: None,
                recomputed_projection_digest: None,
            })
        }
        (None, None) => {
            checks.push(CheckReport {
                id: "result_commitment_shape_recognized",
                ok: false,
                detail: "resultCommitment is neither an ArgsProjection nor an ArgsRef".to_string(),
            });
            Some(ResultCommitmentReport {
                kind: "unrecognized",
                projection_digest: None,
                ref_digest: None,
                embedded_digest: None,
                recomputed_projection_digest: None,
            })
        }
    }
}

fn binding_expectation(
    binding_input: &BindingInput,
    decision_backlink: &BackLinkReport,
    fallback_projection: FallbackProjection,
) -> Result<BindingExpectation> {
    match binding_input {
        BindingInput::Attestation(attestation) => Ok(BindingExpectation {
            mode: "sep2787_attestation",
            digest: Some(jcs_digest(attestation).context("failed to digest attestation")?),
            digest_source: "sep2787_attestation_jcs",
            projection: None,
            named_projection_ready: None,
            named_fail_code: None,
            nonce: string_at(attestation, &["issuerAsserted", "nonce"]),
            nonce_source: "issuerAsserted.nonce",
        }),
        BindingInput::RequestEnvelope(request_envelope) => match fallback_projection {
            FallbackProjection::WholeEnvelope => Ok(BindingExpectation {
                mode: "request_envelope",
                digest: Some(
                    jcs_digest(request_envelope).context("failed to digest request envelope")?,
                ),
                digest_source: "request_envelope_jcs",
                projection: None,
                named_projection_ready: None,
                named_fail_code: None,
                nonce: decision_backlink.attestation_nonce.clone(),
                nonce_source: "record_backlink_consistency",
            }),
            FallbackProjection::Named => {
                let resolved = resolve_named_projection(request_envelope);
                let (digest, named_fail_code) = match resolved {
                    Ok((params, binding)) => {
                        let projected = serde_json::json!({
                            "projection": FALLBACK_PROJECTION_V0,
                            "params": params,
                            "binding": binding,
                        });
                        (
                            Some(
                                jcs_digest(&projected)
                                    .context("failed to digest named fallback projection")?,
                            ),
                            None,
                        )
                    }
                    Err(code) => (None, Some(code)),
                };
                let named_projection_ready = digest.is_some();
                Ok(BindingExpectation {
                    mode: "request_envelope",
                    digest,
                    digest_source: "request_envelope_named_projection_jcs",
                    projection: Some(FALLBACK_PROJECTION_V0),
                    named_projection_ready: Some(named_projection_ready),
                    named_fail_code,
                    nonce: decision_backlink.attestation_nonce.clone(),
                    nonce_source: "record_backlink_consistency",
                })
            }
        },
    }
}

/// Resolve the complete v0 preimage once so validation and digest construction cannot drift.
fn resolve_named_projection(
    request_envelope: &Value,
) -> std::result::Result<(&Value, &Value), &'static str> {
    let params = request_envelope
        .get("params")
        .ok_or("fallback_projection_missing_params")?;
    if !params.is_object() {
        return Err("fallback_projection_invalid_params");
    }

    let meta = request_envelope
        .get("_meta")
        .filter(|meta| meta.is_object())
        .ok_or("fallback_projection_invalid_meta")?;
    let binding = meta
        .get("authorization_binding")
        .ok_or("fallback_projection_missing_authorization_binding")?;
    Ok((params, binding))
}

fn attestation_report(
    binding_input: &BindingInput,
    expectation: &BindingExpectation,
) -> Option<AttestationReport> {
    match binding_input {
        BindingInput::Attestation(_) => Some(AttestationReport {
            digest: expectation
                .digest
                .clone()
                .expect("attestation binding always has a digest"),
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
                expectation.digest.as_deref(),
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
            if let Some(digest) = expectation.digest.as_deref() {
                checks.push(check_eq(
                    "decision_request_envelope_digest_match",
                    decision_backlink.attestation_digest.as_deref(),
                    Some(digest),
                    "decision backLink.attestationDigest matches request-envelope JCS digest",
                ));
            }
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
                expectation.digest.as_deref(),
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
            if let Some(digest) = expectation.digest.as_deref() {
                checks.push(check_eq(
                    "outcome_request_envelope_digest_match",
                    outcome_backlink.attestation_digest.as_deref(),
                    Some(digest),
                    "outcome backLink.attestationDigest matches request-envelope JCS digest",
                ));
            }
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
