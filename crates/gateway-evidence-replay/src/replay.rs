use serde_json::Value;
use thiserror::Error;

use crate::schema::{Coverage, EvidenceBundle, Reason, ReplayResult, Status};

#[derive(Debug, Error)]
pub enum VerifyError {
    #[error("failed to decode JSON evidence: {0}")]
    Json(#[from] serde_json::Error),
}

pub fn verify_json_value(value: Value) -> ReplayResult {
    let Ok(bundle) = serde_json::from_value::<EvidenceBundle>(value) else {
        return ReplayResult::invalid();
    };
    verify_bundle(&bundle)
}

pub fn verify_json_str(value: &str) -> ReplayResult {
    match serde_json::from_str::<Value>(value) {
        Ok(value) => verify_json_value(value),
        Err(_) => ReplayResult::invalid(),
    }
}

pub fn verify_bundle(bundle: &EvidenceBundle) -> ReplayResult {
    if bundle.validate_shape().is_err() {
        return ReplayResult::invalid();
    }

    let Some(ceiling) = bundle.source_class.ceiling() else {
        return ReplayResult::new(Status::Invalid, None, vec![Reason::UnknownSourceClass]);
    };

    let evidence = &bundle.evidence;
    if !evidence.signature_verified || !evidence.runtime_measurement_verified {
        return ReplayResult::new(Status::Incomplete, None, vec![Reason::EvidenceNotVerified]);
    }

    let Some(valid_until) = evidence.attestation_valid_until_utc() else {
        return ReplayResult::new(
            Status::Incomplete,
            None,
            vec![Reason::AttestationFreshnessMissing],
        );
    };
    let Ok(now) = bundle.now_utc() else {
        return ReplayResult::invalid();
    };
    if valid_until < now {
        return ReplayResult::new(Status::Incomplete, None, vec![Reason::AttestationStale]);
    }

    let mut mismatches = Vec::new();
    if evidence.selected_route != bundle.claim.requested_route {
        mismatches.push(Reason::RouteSubstitution);
    }
    if !bundle
        .policy
        .allowed_routes
        .iter()
        .any(|route| route == &evidence.selected_route)
    {
        mismatches.push(Reason::RouteNotAllowed);
    }
    if evidence.fallback_route != bundle.claim.expected_fallback {
        mismatches.push(Reason::FallbackMismatch);
    }
    if let Some(fallback_route) = evidence.fallback_route.as_ref() {
        if !bundle
            .policy
            .allowed_fallbacks
            .iter()
            .any(|allowed| allowed == fallback_route)
        {
            mismatches.push(Reason::FallbackNotAllowed);
        }
    }
    if evidence.endpoint.as_deref() != Some(bundle.policy.endpoint.as_str())
        || evidence.endpoint.as_deref() != Some(bundle.claim.expected_endpoint.as_str())
    {
        mismatches.push(Reason::EndpointMismatch);
    }
    if evidence.policy_hash.as_deref() != Some(bundle.policy.policy_hash.as_str()) {
        mismatches.push(Reason::PolicyHashMismatch);
    }
    if let Some(expected_stream) = bundle.claim.expected_stream_commitment.as_ref() {
        if evidence.stream_commitment.is_some()
            && evidence.stream_commitment.as_deref() != Some(expected_stream.as_str())
        {
            mismatches.push(Reason::StreamCommitmentMismatch);
        }
    }

    if !mismatches.is_empty() {
        return ReplayResult::new(Status::PathMismatch, None, mismatches);
    }

    if evidence.stream_commitment.is_none() {
        return ReplayResult::new(
            Status::Incomplete,
            None,
            vec![Reason::StreamEvidenceMissing],
        );
    }

    if bundle.coverage != Coverage::Complete {
        return ReplayResult::new(Status::Incomplete, None, vec![Reason::CoverageNotComplete]);
    }

    ReplayResult::new(Status::PathVerified, Some(ceiling), Vec::new())
}
