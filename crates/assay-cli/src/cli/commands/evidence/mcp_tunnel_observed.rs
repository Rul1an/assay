use anyhow::{Context, Result};
use clap::{Args, ValueEnum};
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

const SCHEMA: &str = "assay.mcp.tunnel_observed.v0";

const REQUIRED_NON_CLAIMS: &[&str] = &[
    "agent_identity_not_verified_by_tunnel_observation",
    "authorization_not_proven_by_tunnel_observation",
    "policy_outcome_not_inferred_from_transport",
    "tool_result_truth_not_proven",
    "application_outcome_not_proven",
    "upstream_server_trust_not_proven",
    "token_freshness_not_proven",
    "observed_facts_trust_depends_on_observation_point_integrity",
    "route_facts_may_be_asserted_not_mediation_proven",
];

const CLAIMS_NOT_MADE: &[&str] = &[
    "agent_identity_verification",
    "authorization_proof",
    "policy_outcome_proof",
    "route_or_transport_mediation_proof",
    "tool_result_truth",
    "application_outcome_truth",
    "raw_payload_or_raw_auth_retention",
];

#[derive(Debug, Args, Clone)]
pub struct McpTunnelObservedArgs {
    /// MCP tunnel observed-facts artifact JSON fixture
    #[arg(long)]
    pub artifact: PathBuf,

    /// Output format
    #[arg(long, value_enum, default_value_t = McpTunnelObservedFormat::Table)]
    pub format: McpTunnelObservedFormat,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum McpTunnelObservedFormat {
    Json,
    Table,
}

#[derive(Debug, Serialize)]
struct TunnelObservedReport {
    schema: &'static str,
    ok: bool,
    verification_scope: VerificationScope,
    request_binding: RequestBindingReport,
    join_summary: JoinSummary,
    checks: Vec<CheckReport>,
    claims_not_made: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
struct VerificationScope {
    role: &'static str,
    note: &'static str,
}

#[derive(Debug, Serialize)]
struct RequestBindingReport {
    request_envelope_digest: Option<String>,
    request_envelope_canonicalization: Option<String>,
}

#[derive(Debug, Serialize, Default)]
struct JoinSummary {
    strong_same_request_instance: usize,
    diagnostic_correlation: usize,
}

#[derive(Debug, Serialize)]
struct CheckReport {
    id: &'static str,
    ok: bool,
    detail: String,
}

pub fn cmd_verify_mcp_tunnel_observed(args: McpTunnelObservedArgs) -> Result<i32> {
    let artifact = read_json(&args.artifact)?;
    let report = build_report(&artifact);
    match args.format {
        McpTunnelObservedFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        McpTunnelObservedFormat::Table => print_table_report(&report),
    }
    Ok(if report.ok { 0 } else { 2 })
}

fn read_json(path: &PathBuf) -> Result<Value> {
    let body =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&body).with_context(|| format!("failed to parse {}", path.display()))
}

fn build_report(artifact: &Value) -> TunnelObservedReport {
    let request_digest = string_at(artifact, &["request_instance", "request_envelope_digest"]);
    let request_canonicalization = string_at(
        artifact,
        &["request_instance", "request_envelope_canonicalization"],
    );
    let mut checks = Vec::new();
    let mut join_summary = JoinSummary::default();

    checks.push(check_eq(
        "schema",
        string_at(artifact, &["schema"]).as_deref(),
        Some(SCHEMA),
        "artifact schema is the MCP tunnel observed-facts v0 contract",
    ));
    checks.push(check_sha256(
        "request_envelope_digest_shape",
        request_digest.as_deref(),
        "request_instance.request_envelope_digest is a sha256 digest",
    ));
    checks.push(check_present(
        "request_envelope_canonicalization_present",
        request_canonicalization.as_deref(),
        "request_instance.request_envelope_canonicalization is present",
    ));

    push_visibility_checks(artifact, &mut checks);
    push_auth_context_checks(artifact, &mut checks);
    push_inspector_ref_checks(artifact, &mut checks);
    push_non_claim_checks(artifact, &mut checks);
    push_evidence_ref_checks(
        artifact,
        request_digest.as_deref(),
        request_canonicalization.as_deref(),
        &mut checks,
        &mut join_summary,
    );

    let ok = checks.iter().all(|check| check.ok);
    TunnelObservedReport {
        schema: "assay.mcp.tunnel-observed.report.v0",
        ok,
        verification_scope: VerificationScope {
            role: "independent-consumer",
            note: "Assay validates bounded observed facts and join classification only; it does not prove mediation, authorization, identity, policy correctness, or runtime truth.",
        },
        request_binding: RequestBindingReport {
            request_envelope_digest: request_digest,
            request_envelope_canonicalization: request_canonicalization,
        },
        join_summary,
        checks,
        claims_not_made: CLAIMS_NOT_MADE.to_vec(),
    }
}

fn push_visibility_checks(artifact: &Value, checks: &mut Vec<CheckReport>) {
    let visibility = object_at(artifact, &["visibility"]);
    checks.push(CheckReport {
        id: "visibility_object_present",
        ok: visibility.is_some(),
        detail: if visibility.is_some() {
            "visibility object is present".to_string()
        } else {
            "missing visibility object".to_string()
        },
    });
    let Some(visibility) = visibility else {
        return;
    };

    for key in [
        "tool_result_visible",
        "policy_decision_visible",
        "raw_payload_retained",
    ] {
        checks.push(CheckReport {
            id: match key {
                "tool_result_visible" => "tool_result_visible_boolean",
                "policy_decision_visible" => "policy_decision_visible_boolean",
                "raw_payload_retained" => "raw_payload_retained_boolean",
                _ => unreachable!(),
            },
            ok: visibility.get(key).is_some_and(Value::is_boolean),
            detail: format!("visibility.{key} must be a JSON boolean"),
        });
    }

    let raw_payload_retained = visibility
        .get("raw_payload_retained")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    checks.push(CheckReport {
        id: "raw_payload_not_retained",
        ok: !raw_payload_retained,
        detail: "visibility.raw_payload_retained must be false".to_string(),
    });

    for key in ["request_payload_mode", "response_payload_mode"] {
        let value = visibility.get(key).and_then(Value::as_str);
        let allowed = ["not_observed", "digest_only", "redacted_projection"];
        checks.push(CheckReport {
            id: match key {
                "request_payload_mode" => "request_payload_mode_enum",
                "response_payload_mode" => "response_payload_mode_enum",
                _ => unreachable!(),
            },
            ok: value.is_some_and(|mode| allowed.contains(&mode)),
            detail: match value {
                Some(value) if allowed.contains(&value) => format!("{value} is allowed"),
                Some(value) => format!("{value} is not one of {}", allowed.join(", ")),
                None => "missing value".to_string(),
            },
        });
    }
}

fn push_auth_context_checks(artifact: &Value, checks: &mut Vec<CheckReport>) {
    let Some(auth) = object_at(artifact, &["auth_context"]) else {
        checks.push(CheckReport {
            id: "auth_context_absent",
            ok: true,
            detail: "no auth_context supplied".to_string(),
        });
        return;
    };

    let allowed = [
        "authorization_header_visible",
        "authorization_header_stored",
        "authorization_header_digest",
        "mcp_oauth_metadata_visible",
        "client_mtls_configured",
    ];
    let unsupported: Vec<_> = auth
        .keys()
        .filter(|key| !allowed.contains(&key.as_str()))
        .cloned()
        .collect();
    checks.push(CheckReport {
        id: "auth_context_key_allowlist",
        ok: unsupported.is_empty(),
        detail: if unsupported.is_empty() {
            "auth_context contains only bounded keys".to_string()
        } else {
            format!("unsupported auth_context keys: {}", unsupported.join(", "))
        },
    });

    let stored = auth
        .get("authorization_header_stored")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    checks.push(CheckReport {
        id: "raw_authorization_not_stored",
        ok: !stored,
        detail: "authorization_header_stored must be false".to_string(),
    });

    for key in [
        "authorization_header_visible",
        "authorization_header_stored",
        "mcp_oauth_metadata_visible",
        "client_mtls_configured",
    ] {
        if auth.get(key).is_some() {
            checks.push(CheckReport {
                id: match key {
                    "authorization_header_visible" => "authorization_header_visible_boolean",
                    "authorization_header_stored" => "authorization_header_stored_boolean",
                    "mcp_oauth_metadata_visible" => "mcp_oauth_metadata_visible_boolean",
                    "client_mtls_configured" => "client_mtls_configured_boolean",
                    _ => unreachable!(),
                },
                ok: auth.get(key).is_some_and(Value::is_boolean),
                detail: format!("auth_context.{key} must be a JSON boolean"),
            });
        }
    }

    if auth.get("authorization_header_digest").is_some() {
        checks.push(check_sha256(
            "authorization_header_digest_shape",
            auth.get("authorization_header_digest")
                .and_then(Value::as_str),
            "authorization_header_digest is a sha256 digest",
        ));
    }
}

fn push_inspector_ref_checks(artifact: &Value, checks: &mut Vec<CheckReport>) {
    let Some(refs) = artifact
        .get("inspector_event_refs")
        .and_then(Value::as_array)
    else {
        checks.push(CheckReport {
            id: "inspector_event_refs_absent",
            ok: true,
            detail: "no inspector_event_refs supplied".to_string(),
        });
        return;
    };

    for (index, reference) in refs.iter().enumerate() {
        let Some(object) = reference.as_object() else {
            checks.push(CheckReport {
                id: "inspector_event_ref_shape",
                ok: false,
                detail: format!("inspector_event_refs[{index}] must be an object"),
            });
            continue;
        };
        let unsupported: Vec<_> = object
            .keys()
            .filter(|key| !["kind", "digest", "ref"].contains(&key.as_str()))
            .cloned()
            .collect();
        checks.push(CheckReport {
            id: "inspector_event_ref_key_allowlist",
            ok: unsupported.is_empty(),
            detail: if unsupported.is_empty() {
                format!("inspector_event_refs[{index}] contains only bounded keys")
            } else {
                format!(
                    "inspector_event_refs[{index}] unsupported keys: {}",
                    unsupported.join(", ")
                )
            },
        });
        checks.push(check_sha256(
            "inspector_event_ref_digest_shape",
            object.get("digest").and_then(Value::as_str),
            "inspector_event_refs[].digest is a sha256 digest",
        ));
    }
}

fn push_non_claim_checks(artifact: &Value, checks: &mut Vec<CheckReport>) {
    let supplied: Vec<&str> = artifact
        .get("non_claims")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let missing: Vec<_> = REQUIRED_NON_CLAIMS
        .iter()
        .copied()
        .filter(|claim| !supplied.contains(claim))
        .collect();
    checks.push(CheckReport {
        id: "required_non_claims_present",
        ok: missing.is_empty(),
        detail: if missing.is_empty() {
            "all required non-claims are present".to_string()
        } else {
            format!("missing non-claims: {}", missing.join(", "))
        },
    });
}

fn push_evidence_ref_checks(
    artifact: &Value,
    request_digest: Option<&str>,
    request_canonicalization: Option<&str>,
    checks: &mut Vec<CheckReport>,
    join_summary: &mut JoinSummary,
) {
    let Some(refs) = artifact.get("evidence_refs").and_then(Value::as_array) else {
        checks.push(CheckReport {
            id: "evidence_refs_absent",
            ok: true,
            detail: "no evidence_refs supplied".to_string(),
        });
        return;
    };

    for (index, reference) in refs.iter().enumerate() {
        let relationship = string_at(reference, &["relationship"]);
        let join_strength = string_at(reference, &["join_strength"]);
        let ref_digest = string_at(reference, &["request_envelope_digest"]);
        let ref_canonicalization = string_at(reference, &["request_envelope_canonicalization"]);
        let is_strong_same_request = relationship.as_deref() == Some("same_request_instance")
            && join_strength.as_deref() == Some("strong");

        if is_strong_same_request {
            join_summary.strong_same_request_instance += 1;
            let ok = ref_digest.as_deref() == request_digest
                && ref_canonicalization.as_deref() == request_canonicalization
                && request_digest.is_some()
                && request_canonicalization.is_some();
            checks.push(CheckReport {
                id: "same_request_instance_strong_join",
                ok,
                detail: if ok {
                    format!(
                        "evidence_refs[{index}] binds the same request envelope digest and canonicalization"
                    )
                } else {
                    format!(
                        "evidence_refs[{index}] strong join requires matching request_envelope_digest and request_envelope_canonicalization"
                    )
                },
            });
        } else {
            join_summary.diagnostic_correlation += 1;
            checks.push(CheckReport {
                id: "diagnostic_correlation_boundary",
                ok: true,
                detail: format!(
                    "evidence_refs[{index}] is diagnostic because it is not a strong shared request-envelope join"
                ),
            });
        }
    }
}

fn object_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a serde_json::Map<String, Value>> {
    let mut current = value;
    for segment in path {
        current = current.get(*segment)?;
    }
    current.as_object()
}

fn string_at(value: &Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for segment in path {
        current = current.get(*segment)?;
    }
    current.as_str().map(ToOwned::to_owned)
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

fn check_sha256(id: &'static str, value: Option<&str>, description: &str) -> CheckReport {
    let ok = value.is_some_and(is_sha256_digest);
    let detail = match value {
        Some(value) if is_sha256_digest(value) => description.to_string(),
        Some(value) => format!("{value} is not sha256:<64 lowercase hex chars>"),
        None => "missing value".to_string(),
    };
    CheckReport { id, ok, detail }
}

fn is_sha256_digest(value: &str) -> bool {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return false;
    };
    hex.len() == 64
        && hex
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn print_table_report(report: &TunnelObservedReport) {
    println!("MCP Tunnel Observed-Facts Report");
    println!("================================");
    println!(
        "OK:                  {}",
        if report.ok { "yes" } else { "no" }
    );
    println!("Role:                {}", report.verification_scope.role);
    println!(
        "Request digest:      {}",
        report
            .request_binding
            .request_envelope_digest
            .as_deref()
            .unwrap_or("-")
    );
    println!(
        "Request canon:       {}",
        report
            .request_binding
            .request_envelope_canonicalization
            .as_deref()
            .unwrap_or("-")
    );
    println!(
        "Strong joins:        {}",
        report.join_summary.strong_same_request_instance
    );
    println!(
        "Diagnostic joins:    {}",
        report.join_summary.diagnostic_correlation
    );
    println!();
    for check in &report.checks {
        println!(
            "{:<42} {:<4} {}",
            check.id,
            if check.ok { "ok" } else { "fail" },
            check.detail
        );
    }
    println!();
    println!("Claims not made: {}", report.claims_not_made.join(", "));
}
