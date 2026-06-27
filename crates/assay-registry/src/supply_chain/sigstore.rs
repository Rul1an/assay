use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

use crate::rekor::{verify_rekor_v2_inclusion_offline, TransparencyRequirement};
use crate::sigstore_identity::{verify_identity_offline, ExpectedIdentity};
use crate::sigstore_offline::verify_cert_chain_offline;
use crate::sigstore_signature::{
    bind_in_toto_subject_digest, verify_leaf_ecdsa_signature_over_bytes,
};

use super::provenance::{build_pae, InTotoStatement, ProvenanceOutcome};
use super::types::*;
use super::{hex_of, DSSE_PAYLOAD_TYPE, STATEMENT_TYPE_V1};

/// A Sigstore bundle parsed exactly ONCE into neutral evidence: every Sigstore dimension is computed
/// from the SAME bytes, so `identity` can never read one leaf while `rekor` binds another.
struct ParsedSigstoreBundleEvidence {
    leaf_der: Vec<u8>,
    payload_type: Option<String>,
    statement_payload: Vec<u8>,
    dsse_signature: Option<Vec<u8>>,
}

const BUNDLE_MEDIA_TYPE_V0_3: &str = "application/vnd.dev.sigstore.bundle.v0.3+json";

/// Shape/availability gate for the Sigstore DSSE path. Returns neutral evidence on success, or the
/// `sigstore_bundle` status to record on failure.
fn parse_sigstore_bundle(bundle_json: &[u8]) -> Result<ParsedSigstoreBundleEvidence, CheckStatus> {
    let bundle: serde_json::Value =
        serde_json::from_slice(bundle_json).map_err(|_| CheckStatus::Failed)?;
    if bundle.get("mediaType").and_then(|v| v.as_str()) != Some(BUNDLE_MEDIA_TYPE_V0_3) {
        return Err(CheckStatus::UnsupportedFormat);
    }
    if bundle.get("messageSignature").is_some() {
        return Err(CheckStatus::UnsupportedFormat);
    }
    let dsse = match bundle.get("dsseEnvelope").filter(|v| v.is_object()) {
        Some(v) => v,
        None => return Err(CheckStatus::Failed),
    };
    let material = match bundle.get("verificationMaterial") {
        Some(v) => v,
        None => return Err(CheckStatus::Failed),
    };
    if material.get("x509CertificateChain").is_some() || material.get("publicKey").is_some() {
        return Err(CheckStatus::UnsupportedFormat);
    }
    let raw_bytes = match material
        .pointer("/certificate/rawBytes")
        .and_then(|v| v.as_str())
    {
        Some(s) => s,
        None => return Err(CheckStatus::UnsupportedFormat),
    };
    let leaf_der = BASE64
        .decode(raw_bytes.as_bytes())
        .map_err(|_| CheckStatus::Failed)?;
    let payload_type = dsse
        .get("payloadType")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let statement_payload = dsse
        .get("payload")
        .and_then(|v| v.as_str())
        .and_then(|p| BASE64.decode(p.as_bytes()).ok())
        .unwrap_or_default();
    let dsse_signature = match dsse.get("signatures").and_then(|v| v.as_array()) {
        Some(sigs) if sigs.len() == 1 => sigs[0]
            .get("sig")
            .and_then(|v| v.as_str())
            .and_then(|s| BASE64.decode(s.as_bytes()).ok()),
        _ => None,
    };

    Ok(ParsedSigstoreBundleEvidence {
        leaf_der,
        payload_type,
        statement_payload,
        dsse_signature,
    })
}

/// Compose the offline Sigstore-keyless primitives (chain, identity, DSSE/PAE, subject binding, Rekor
/// v2 inclusion) into orthogonal provenance dimensions from a single shared parse of the bundle.
pub(super) fn verify_sigstore_bundle_provenance(
    sb: &SigstoreBundleInput,
    subject: &Subject,
    policy: &Policy,
) -> ProvenanceOutcome {
    let na = CheckStatus::NotApplicable;
    let want = hex_of(&subject.digest);
    let rekor_requirement = if policy.require_rekor_inclusion {
        TransparencyRequirement::Required
    } else {
        TransparencyRequirement::Optional
    };
    match parse_sigstore_bundle(&sb.bundle_json) {
        Err(status) => ProvenanceOutcome {
            checks: ProvenanceChecks {
                dsse_signature: na,
                slsa_provenance: na,
                builder_identity: na,
                sigstore_bundle: status,
                rekor_inclusion: status,
                cert_chain: status,
                identity: status,
                dsse_pae: status,
                timestamp_freshness: CheckStatus::NotChecked,
                consistency: CheckStatus::NotChecked,
                witnessing: CheckStatus::NotChecked,
            },
            subject_digest_binding: status,
            verified_level: SlsaLevel(0),
        },
        Ok(ev) => {
            let roots: Vec<&[u8]> = sb.fulcio_roots.iter().map(|v| v.as_slice()).collect();
            let inters: Vec<&[u8]> = sb
                .fulcio_intermediates
                .iter()
                .map(|v| v.as_slice())
                .collect();
            let expected = ExpectedIdentity {
                san: &sb.expected_san,
                issuer: &sb.expected_issuer,
            };

            let cert_chain =
                verify_cert_chain_offline(&ev.leaf_der, &inters, &roots, sb.now_unix_secs).status;
            let identity =
                verify_identity_offline(&ev.leaf_der, &inters, &roots, sb.now_unix_secs, &expected)
                    .status;
            let dsse_pae = match (&ev.payload_type, &ev.dsse_signature) {
                (Some(pt), Some(sig))
                    if pt == DSSE_PAYLOAD_TYPE && !ev.statement_payload.is_empty() =>
                {
                    let pae = build_pae(pt, &ev.statement_payload);
                    verify_leaf_ecdsa_signature_over_bytes(&ev.leaf_der, &pae, sig).status
                }
                _ => CheckStatus::UnsupportedFormat,
            };
            let subject_digest_binding = match ev.payload_type.as_deref() {
                Some(DSSE_PAYLOAD_TYPE) => {
                    match serde_json::from_slice::<InTotoStatement>(&ev.statement_payload) {
                        Ok(s) if s.type_ == STATEMENT_TYPE_V1 => {
                            bind_in_toto_subject_digest(&ev.statement_payload, want).status
                        }
                        Ok(_) => CheckStatus::UnsupportedFormat,
                        Err(_) => CheckStatus::Failed,
                    }
                }
                _ => CheckStatus::UnsupportedFormat,
            };
            let rekor_inclusion = verify_rekor_v2_inclusion_offline(
                &sb.bundle_json,
                &sb.rekor_trusted_root_json,
                rekor_requirement,
            )
            .status;

            ProvenanceOutcome {
                checks: ProvenanceChecks {
                    dsse_signature: na,
                    slsa_provenance: na,
                    builder_identity: na,
                    sigstore_bundle: CheckStatus::Verified,
                    rekor_inclusion,
                    cert_chain,
                    identity,
                    dsse_pae,
                    timestamp_freshness: CheckStatus::NotChecked,
                    consistency: CheckStatus::NotChecked,
                    witnessing: CheckStatus::NotChecked,
                },
                subject_digest_binding,
                verified_level: SlsaLevel(0),
            }
        }
    }
}
