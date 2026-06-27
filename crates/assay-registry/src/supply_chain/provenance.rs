use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ed25519_dalek::{Signature, Verifier};
use serde::Deserialize;

use crate::trust::TrustStore;
use crate::types::DsseEnvelope;

use super::sigstore::verify_sigstore_bundle_provenance;
use super::types::*;
use super::{hex_of, DSSE_PAYLOAD_TYPE, SLSA_PROVENANCE_PREDICATE, STATEMENT_TYPE_V1};

// ---- in-toto / SLSA parsing (serde_json, no new dep) --------------------------------------------

#[derive(Deserialize)]
pub(super) struct InTotoStatement {
    #[serde(rename = "_type")]
    pub(super) type_: String,
    subject: Vec<InTotoSubject>,
    #[serde(rename = "predicateType")]
    predicate_type: String,
    #[serde(default)]
    predicate: serde_json::Value,
}

#[derive(Deserialize)]
struct InTotoSubject {
    #[serde(default)]
    digest: std::collections::BTreeMap<String, String>,
}

// ---- Verification --------------------------------------------------------------------------------

pub(super) fn build_pae(payload_type: &str, payload: &[u8]) -> Vec<u8> {
    let mut pae = Vec::new();
    pae.extend_from_slice(b"DSSEv1 ");
    pae.extend_from_slice(payload_type.len().to_string().as_bytes());
    pae.push(b' ');
    pae.extend_from_slice(payload_type.as_bytes());
    pae.push(b' ');
    pae.extend_from_slice(payload.len().to_string().as_bytes());
    pae.push(b' ');
    pae.extend_from_slice(payload);
    pae
}

/// Verify a DSSE envelope's signatures against the pinned trust store. Distinguishes "no trusted key
/// for this key id" (`TrustRootUnavailable`) from "key found but signature invalid" (`Failed`).
fn verify_dsse_signature(envelope: &DsseEnvelope, trust_store: &TrustStore) -> CheckStatus {
    if envelope.payload_type != DSSE_PAYLOAD_TYPE {
        return CheckStatus::UnsupportedFormat;
    }
    let payload_bytes = match BASE64.decode(&envelope.payload) {
        Ok(b) => b,
        Err(_) => return CheckStatus::Failed,
    };
    if envelope.signatures.is_empty() {
        return CheckStatus::NotPresent;
    }
    let pae = build_pae(&envelope.payload_type, &payload_bytes);
    let mut any_key_found = false;
    for sig in &envelope.signatures {
        let key = match trust_store.get_key(&sig.key_id) {
            Ok(k) => k,
            Err(_) => continue,
        };
        any_key_found = true;
        let sig_bytes = match BASE64.decode(&sig.signature) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let signature = match Signature::from_slice(&sig_bytes) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if key.verify(&pae, &signature).is_ok() {
            return CheckStatus::Verified;
        }
    }
    if any_key_found {
        CheckStatus::Failed
    } else {
        CheckStatus::TrustRootUnavailable
    }
}

fn decode_statement(envelope: &DsseEnvelope) -> Option<InTotoStatement> {
    let payload = BASE64.decode(&envelope.payload).ok()?;
    serde_json::from_slice::<InTotoStatement>(&payload).ok()
}

/// The whole provenance check group + the verified SLSA level.
pub(super) struct ProvenanceOutcome {
    pub(super) checks: ProvenanceChecks,
    pub(super) subject_digest_binding: CheckStatus,
    pub(super) verified_level: SlsaLevel,
}

pub(super) fn verify_provenance(input: &VerifyInput<'_>) -> ProvenanceOutcome {
    let na = CheckStatus::NotApplicable;
    match &input.provenance {
        ProvenanceInput::None => ProvenanceOutcome {
            checks: ProvenanceChecks {
                dsse_signature: CheckStatus::NotPresent,
                slsa_provenance: CheckStatus::NotPresent,
                builder_identity: CheckStatus::NotPresent,
                sigstore_bundle: CheckStatus::NotPresent,
                rekor_inclusion: CheckStatus::NotPresent,
                cert_chain: CheckStatus::NotPresent,
                identity: CheckStatus::NotPresent,
                dsse_pae: CheckStatus::NotPresent,
                timestamp_freshness: na,
                consistency: na,
                witnessing: na,
            },
            subject_digest_binding: CheckStatus::NotPresent,
            verified_level: SlsaLevel(0),
        },
        ProvenanceInput::SigstoreBundle(sb) => {
            verify_sigstore_bundle_provenance(sb, &input.subject, &input.policy)
        }
        ProvenanceInput::Unsupported(kind) => unsupported_provenance(*kind),
        ProvenanceInput::Dsse(env) => verify_pinned_key_dsse(input, env),
    }
}

fn unsupported_provenance(kind: UnsupportedProvenance) -> ProvenanceOutcome {
    let na = CheckStatus::NotApplicable;
    let sigstore = match kind {
        UnsupportedProvenance::Pep740 | UnsupportedProvenance::NpmProvenance => {
            CheckStatus::UnsupportedFormat
        }
        UnsupportedProvenance::UnknownPredicate => CheckStatus::NotApplicable,
    };
    ProvenanceOutcome {
        checks: ProvenanceChecks {
            dsse_signature: CheckStatus::UnsupportedFormat,
            slsa_provenance: CheckStatus::UnsupportedFormat,
            builder_identity: CheckStatus::UnsupportedFormat,
            sigstore_bundle: sigstore,
            rekor_inclusion: CheckStatus::NotApplicable,
            cert_chain: CheckStatus::UnsupportedFormat,
            identity: CheckStatus::UnsupportedFormat,
            dsse_pae: CheckStatus::UnsupportedFormat,
            timestamp_freshness: na,
            consistency: na,
            witnessing: na,
        },
        subject_digest_binding: CheckStatus::NotApplicable,
        verified_level: SlsaLevel(0),
    }
}

fn verify_pinned_key_dsse(input: &VerifyInput<'_>, env: &DsseEnvelope) -> ProvenanceOutcome {
    let na = CheckStatus::NotApplicable;
    let dsse_signature = verify_dsse_signature(env, input.trust_store);
    let statement = decode_statement(env);
    let want = hex_of(&input.subject.digest);
    let subject_digest_binding = match &statement {
        Some(s) if s.type_ == STATEMENT_TYPE_V1 => {
            let bound = s
                .subject
                .iter()
                .filter_map(|sub| sub.digest.get("sha256"))
                .any(|d| hex_of(d) == want);
            if bound {
                CheckStatus::Verified
            } else {
                CheckStatus::SubjectDigestMismatch
            }
        }
        _ => CheckStatus::Failed,
    };
    let is_slsa = statement
        .as_ref()
        .map(|s| s.predicate_type == SLSA_PROVENANCE_PREDICATE)
        .unwrap_or(false);
    let builder_id = statement
        .as_ref()
        .and_then(|s| s.predicate.pointer("/runDetails/builder/id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let builder_identity = match (&input.policy.required_builder_id, &builder_id) {
        _ if !is_slsa => CheckStatus::UnsupportedFormat,
        (Some(req), Some(got)) if req == got => CheckStatus::Verified,
        (Some(_), Some(_)) => CheckStatus::IdentityMismatch,
        (Some(_), None) => CheckStatus::IdentityMismatch,
        (None, Some(_)) => CheckStatus::Verified,
        (None, None) => na,
    };
    let binds = subject_digest_binding == CheckStatus::Verified;
    let signed_ok = dsse_signature == CheckStatus::Verified;
    let identity_ok = matches!(
        builder_identity,
        CheckStatus::Verified | CheckStatus::NotApplicable
    );
    let verified_level = if !is_slsa || !binds {
        SlsaLevel(0)
    } else if signed_ok && identity_ok {
        SlsaLevel(2)
    } else {
        SlsaLevel(1)
    };
    let required = input.policy.required_slsa_build_level;
    let slsa_provenance = if !is_slsa {
        CheckStatus::UnsupportedFormat
    } else if verified_level >= required {
        CheckStatus::Verified
    } else {
        CheckStatus::Failed
    };
    ProvenanceOutcome {
        checks: ProvenanceChecks {
            dsse_signature,
            slsa_provenance,
            builder_identity,
            sigstore_bundle: CheckStatus::NotApplicable,
            rekor_inclusion: CheckStatus::NotApplicable,
            cert_chain: na,
            identity: na,
            dsse_pae: na,
            timestamp_freshness: na,
            consistency: na,
            witnessing: na,
        },
        subject_digest_binding,
        verified_level,
    }
}
