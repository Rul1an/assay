//! MCP04a-3.3a — DSSE envelope / PAE verification primitive.
//!
//! Verifies a DSSE (Dead Simple Signing Envelope) carrying an in-toto Statement, fully offline. DSSE's
//! security property is that the signature covers a **Pre-Authentication Encoding (PAE)** of
//! `payloadType` + `payload`, NOT the raw payload bytes — this is what prevents payload-type confusion.
//! a-3.2b's raw-ECDSA-over-bytes primitive is therefore **not** DSSE verification; this slice constructs
//! the PAE and verifies over it, so a signature made over the raw payload (or over a different
//! payloadType) is rejected.
//!
//! Scope (a-3.3a): parse envelope -> exactly one signature -> supported payloadType -> base64 decode ->
//! build the DSSEv1 PAE -> verify a DER ECDSA/P-256 signature over the PAE under the leaf key -> bind the
//! payload's in-toto subject digest (via the a-3.2b binder).
//!
//! NOT in scope: Sigstore bundle composition (a-3.3b), Rekor inclusion (a-3.3c), PEP740/npm adapters,
//! carrier integration, chain/identity composition. **DSSE verified is NOT predicate-policy satisfied.**

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

use crate::sigstore_signature::{
    bind_in_toto_subject_digest, verify_leaf_ecdsa_signature_over_bytes,
};
use crate::supply_chain::CheckStatus;

/// The only DSSE payloadType this slice accepts: in-toto Statements.
const IN_TOTO_PAYLOAD_TYPE: &str = "application/vnd.in-toto+json";

/// The outcome of offline DSSE verification: a `CheckStatus` plus a value-free reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DsseOutcome {
    pub status: CheckStatus,
    pub reason: &'static str,
}

impl DsseOutcome {
    fn new(status: CheckStatus, reason: &'static str) -> Self {
        Self { status, reason }
    }
}

/// Lenient DSSE envelope shape for parsing. Fields are optional so a missing one yields a precise
/// `UnsupportedFormat` rather than a generic parse error; `keyid` is intentionally ignored (cert-based
/// DSSE often omits it).
#[derive(serde::Deserialize)]
struct EnvelopeIn {
    #[serde(rename = "payloadType")]
    payload_type: Option<String>,
    payload: Option<String>,
    #[serde(default)]
    signatures: Vec<SignatureIn>,
}

#[derive(serde::Deserialize)]
struct SignatureIn {
    sig: Option<String>,
}

/// Build the DSSE v1 Pre-Authentication Encoding:
/// `"DSSEv1" SP LEN(payloadType) SP payloadType SP LEN(payload) SP payload`, where SP is a single ASCII
/// space and LEN is the ASCII-decimal byte length. The signature is computed over exactly these bytes.
fn dsse_pae(payload_type: &str, payload: &[u8]) -> Vec<u8> {
    let mut pae = Vec::with_capacity(payload.len() + payload_type.len() + 32);
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

/// Verify a DSSE envelope over an in-toto Statement, fully offline, using the public key in `leaf_der`.
///
/// The signature is verified over the DSSE **PAE** (not the raw payload), so payload-type confusion and
/// raw-payload signatures are rejected. The decoded payload is bound to `expected_sha256` via the in-toto
/// subject binder. This does NOT parse a Sigstore bundle, check identity/chain, or touch Rekor.
pub fn verify_dsse_envelope_offline(
    leaf_der: &[u8],
    envelope_json: &[u8],
    expected_sha256: &str,
) -> DsseOutcome {
    let envelope: EnvelopeIn = match serde_json::from_slice(envelope_json) {
        Ok(e) => e,
        Err(_) => {
            return DsseOutcome::new(CheckStatus::UnsupportedFormat, "malformed DSSE envelope")
        }
    };

    let payload_type = match envelope.payload_type {
        Some(t) => t,
        None => {
            return DsseOutcome::new(
                CheckStatus::UnsupportedFormat,
                "DSSE envelope is missing payloadType",
            )
        }
    };
    let payload_b64 = match envelope.payload {
        Some(p) => p,
        None => {
            return DsseOutcome::new(
                CheckStatus::UnsupportedFormat,
                "DSSE envelope is missing payload",
            )
        }
    };
    // Exactly one signature for this slice (covers both missing and multiple).
    let signature_entry = match envelope.signatures.as_slice() {
        [only] => only,
        _ => {
            return DsseOutcome::new(
                CheckStatus::UnsupportedFormat,
                "DSSE envelope does not carry exactly one signature",
            )
        }
    };
    let sig_b64 = match &signature_entry.sig {
        Some(s) => s,
        None => {
            return DsseOutcome::new(
                CheckStatus::UnsupportedFormat,
                "DSSE signature entry is missing sig",
            )
        }
    };

    if payload_type != IN_TOTO_PAYLOAD_TYPE {
        return DsseOutcome::new(
            CheckStatus::UnsupportedFormat,
            "unsupported DSSE payloadType",
        );
    }

    let payload = match BASE64.decode(payload_b64.as_bytes()) {
        Ok(b) => b,
        Err(_) => {
            return DsseOutcome::new(
                CheckStatus::UnsupportedFormat,
                "DSSE payload is not valid base64",
            )
        }
    };
    let signature_der = match BASE64.decode(sig_b64.as_bytes()) {
        Ok(b) => b,
        Err(_) => {
            return DsseOutcome::new(
                CheckStatus::UnsupportedFormat,
                "DSSE signature is not valid base64",
            )
        }
    };

    // The signature is over the PAE, never the raw payload — this is the anti-confusion layer.
    let pae = dsse_pae(&payload_type, &payload);
    let signature = verify_leaf_ecdsa_signature_over_bytes(leaf_der, &pae, &signature_der);
    if signature.status != CheckStatus::Verified {
        return DsseOutcome::new(signature.status, signature.reason);
    }

    // The decoded payload is the in-toto Statement; bind its subject digest.
    let digest = bind_in_toto_subject_digest(&payload, expected_sha256);
    if digest.status != CheckStatus::Verified {
        return DsseOutcome::new(digest.status, digest.reason);
    }

    DsseOutcome::new(
        CheckStatus::Verified,
        "DSSE PAE signature verifies and payload subject digest matches",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ecdsa::signature::Signer;
    use p256::ecdsa::{Signature, SigningKey};
    use p256::pkcs8::DecodePrivateKey;
    use rcgen::{CertificateParams, KeyPair, PKCS_ECDSA_P256_SHA256};

    const ARTIFACT_SHA256: &str =
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    /// A self-signed P-256 leaf and its keypair. a-3.3a only needs the leaf's public key (for the
    /// signature) — chain/identity composition is a-3.3b.
    fn leaf_with_key() -> (Vec<u8>, KeyPair) {
        let key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).unwrap();
        let params = CertificateParams::new(Vec::<String>::new()).unwrap();
        let cert = params.self_signed(&key).unwrap();
        (cert.der().to_vec(), key)
    }

    fn sign(key: &KeyPair, msg: &[u8]) -> Vec<u8> {
        let sk = SigningKey::from_pkcs8_der(&key.serialize_der()).unwrap();
        let sig: Signature = sk.sign(msg);
        sig.to_der().as_bytes().to_vec()
    }

    fn statement(sha256: &str) -> Vec<u8> {
        format!(
            r#"{{"_type":"https://in-toto.io/Statement/v1","subject":[{{"name":"artifact","digest":{{"sha256":"{sha256}"}}}}],"predicateType":"x","predicate":{{}}}}"#
        )
        .into_bytes()
    }

    /// Assemble a DSSE envelope JSON with one signature.
    fn envelope(payload_type: &str, payload: &[u8], signature_der: &[u8]) -> Vec<u8> {
        format!(
            r#"{{"payloadType":"{}","payload":"{}","signatures":[{{"keyid":"","sig":"{}"}}]}}"#,
            payload_type,
            BASE64.encode(payload),
            BASE64.encode(signature_der)
        )
        .into_bytes()
    }

    #[test]
    fn valid_dsse_envelope_verifies() {
        let (leaf, key) = leaf_with_key();
        let payload = statement(ARTIFACT_SHA256);
        let sig = sign(&key, &dsse_pae(IN_TOTO_PAYLOAD_TYPE, &payload));
        let env = envelope(IN_TOTO_PAYLOAD_TYPE, &payload, &sig);
        let out = verify_dsse_envelope_offline(&leaf, &env, ARTIFACT_SHA256);
        assert_eq!(out.status, CheckStatus::Verified, "{}", out.reason);
    }

    /// THE load-bearing test: a signature over the RAW payload (not the PAE) must fail DSSE verification,
    /// even though that same signature is a perfectly valid raw-bytes signature. This proves the PAE is
    /// load-bearing and that the a-3.2b raw primitive was not reused as if it were DSSE.
    #[test]
    fn signature_over_raw_payload_not_pae_fails() {
        let (leaf, key) = leaf_with_key();
        let payload = statement(ARTIFACT_SHA256);
        let raw_sig = sign(&key, &payload); // signed the raw payload, NOT the PAE
        let env = envelope(IN_TOTO_PAYLOAD_TYPE, &payload, &raw_sig);

        // DSSE verification fails because the signature is not over the PAE.
        let out = verify_dsse_envelope_offline(&leaf, &env, ARTIFACT_SHA256);
        assert_eq!(out.status, CheckStatus::Failed, "{}", out.reason);

        // ...yet the very same signature IS valid over the raw payload bytes. The difference is the PAE.
        let raw = verify_leaf_ecdsa_signature_over_bytes(&leaf, &payload, &raw_sig);
        assert_eq!(raw.status, CheckStatus::Verified, "{}", raw.reason);
    }

    /// payloadType is bound into the signed PAE: a signature computed over a PAE with a different
    /// payloadType must fail even when the envelope declares the supported type and the payload is
    /// unchanged.
    #[test]
    fn payload_type_bound_into_pae() {
        let (leaf, key) = leaf_with_key();
        let payload = statement(ARTIFACT_SHA256);
        let sig_over_other_type = sign(&key, &dsse_pae("application/vnd.other+json", &payload));
        let env = envelope(IN_TOTO_PAYLOAD_TYPE, &payload, &sig_over_other_type);
        let out = verify_dsse_envelope_offline(&leaf, &env, ARTIFACT_SHA256);
        assert_eq!(out.status, CheckStatus::Failed, "{}", out.reason);
    }

    #[test]
    fn tampered_payload_fails() {
        let (leaf, key) = leaf_with_key();
        let signed = statement(ARTIFACT_SHA256);
        let sig = sign(&key, &dsse_pae(IN_TOTO_PAYLOAD_TYPE, &signed));
        // Swap in a different payload while keeping the original signature.
        let tampered =
            statement("0000000000000000000000000000000000000000000000000000000000000000");
        let env = envelope(IN_TOTO_PAYLOAD_TYPE, &tampered, &sig);
        let out = verify_dsse_envelope_offline(&leaf, &env, ARTIFACT_SHA256);
        assert_eq!(out.status, CheckStatus::Failed, "{}", out.reason);
    }

    #[test]
    fn malformed_base64_payload_is_unsupported_format() {
        let (leaf, key) = leaf_with_key();
        let payload = statement(ARTIFACT_SHA256);
        let sig = sign(&key, &dsse_pae(IN_TOTO_PAYLOAD_TYPE, &payload));
        let env = format!(
            r#"{{"payloadType":"{IN_TOTO_PAYLOAD_TYPE}","payload":"!!!not base64!!!","signatures":[{{"sig":"{}"}}]}}"#,
            BASE64.encode(&sig)
        );
        let out = verify_dsse_envelope_offline(&leaf, env.as_bytes(), ARTIFACT_SHA256);
        assert_eq!(out.status, CheckStatus::UnsupportedFormat, "{}", out.reason);
    }

    #[test]
    fn malformed_base64_signature_is_unsupported_format() {
        let (leaf, _key) = leaf_with_key();
        let payload = statement(ARTIFACT_SHA256);
        let env = format!(
            r#"{{"payloadType":"{IN_TOTO_PAYLOAD_TYPE}","payload":"{}","signatures":[{{"sig":"!!!"}}]}}"#,
            BASE64.encode(&payload)
        );
        let out = verify_dsse_envelope_offline(&leaf, env.as_bytes(), ARTIFACT_SHA256);
        assert_eq!(out.status, CheckStatus::UnsupportedFormat, "{}", out.reason);
    }

    #[test]
    fn unsupported_payload_type_is_unsupported_format() {
        let (leaf, key) = leaf_with_key();
        let payload = statement(ARTIFACT_SHA256);
        let sig = sign(&key, &dsse_pae("application/vnd.other+json", &payload));
        let env = envelope("application/vnd.other+json", &payload, &sig);
        let out = verify_dsse_envelope_offline(&leaf, &env, ARTIFACT_SHA256);
        assert_eq!(out.status, CheckStatus::UnsupportedFormat, "{}", out.reason);
    }

    #[test]
    fn multiple_signatures_is_unsupported_format() {
        let (leaf, key) = leaf_with_key();
        let payload = statement(ARTIFACT_SHA256);
        let sig = sign(&key, &dsse_pae(IN_TOTO_PAYLOAD_TYPE, &payload));
        let env = format!(
            r#"{{"payloadType":"{IN_TOTO_PAYLOAD_TYPE}","payload":"{}","signatures":[{{"sig":"{}"}},{{"sig":"{}"}}]}}"#,
            BASE64.encode(&payload),
            BASE64.encode(&sig),
            BASE64.encode(&sig)
        );
        let out = verify_dsse_envelope_offline(&leaf, env.as_bytes(), ARTIFACT_SHA256);
        assert_eq!(out.status, CheckStatus::UnsupportedFormat, "{}", out.reason);
    }

    #[test]
    fn missing_payload_type_is_unsupported_format() {
        let (leaf, _key) = leaf_with_key();
        let payload = statement(ARTIFACT_SHA256);
        let env = format!(
            r#"{{"payload":"{}","signatures":[{{"sig":"AA=="}}]}}"#,
            BASE64.encode(&payload)
        );
        let out = verify_dsse_envelope_offline(&leaf, env.as_bytes(), ARTIFACT_SHA256);
        assert_eq!(out.status, CheckStatus::UnsupportedFormat, "{}", out.reason);
    }

    #[test]
    fn missing_signatures_is_unsupported_format() {
        let (leaf, _key) = leaf_with_key();
        let payload = statement(ARTIFACT_SHA256);
        let env = format!(
            r#"{{"payloadType":"{IN_TOTO_PAYLOAD_TYPE}","payload":"{}","signatures":[]}}"#,
            BASE64.encode(&payload)
        );
        let out = verify_dsse_envelope_offline(&leaf, env.as_bytes(), ARTIFACT_SHA256);
        assert_eq!(out.status, CheckStatus::UnsupportedFormat, "{}", out.reason);
    }

    #[test]
    fn subject_mismatch_in_payload_is_subject_digest_mismatch() {
        let (leaf, key) = leaf_with_key();
        // The DSSE signature is valid, but the statement commits to a different digest than expected.
        let payload = statement("0000000000000000000000000000000000000000000000000000000000000000");
        let sig = sign(&key, &dsse_pae(IN_TOTO_PAYLOAD_TYPE, &payload));
        let env = envelope(IN_TOTO_PAYLOAD_TYPE, &payload, &sig);
        let out = verify_dsse_envelope_offline(&leaf, &env, ARTIFACT_SHA256);
        assert_eq!(
            out.status,
            CheckStatus::SubjectDigestMismatch,
            "{}",
            out.reason
        );
    }

    /// `keyid` is a non-authoritative hint in DSSE: verification depends ONLY on the supplied leaf SPKI.
    /// A bogus keyid, or no keyid at all, must not change the verdict and must never grant trust or
    /// produce an identity_mismatch.
    #[test]
    fn keyid_is_ignored_as_non_authoritative_hint() {
        let (leaf, key) = leaf_with_key();
        let payload = statement(ARTIFACT_SHA256);
        let sig = sign(&key, &dsse_pae(IN_TOTO_PAYLOAD_TYPE, &payload));

        // (a) a bogus, non-matching keyid still verifies against the leaf key.
        let env_bogus = format!(
            r#"{{"payloadType":"{IN_TOTO_PAYLOAD_TYPE}","payload":"{}","signatures":[{{"keyid":"bogus-non-matching-keyid","sig":"{}"}}]}}"#,
            BASE64.encode(&payload),
            BASE64.encode(&sig)
        );
        let out = verify_dsse_envelope_offline(&leaf, env_bogus.as_bytes(), ARTIFACT_SHA256);
        assert_eq!(out.status, CheckStatus::Verified, "{}", out.reason);

        // (b) keyid absent entirely still verifies (cert-based DSSE often omits it).
        let env_absent = format!(
            r#"{{"payloadType":"{IN_TOTO_PAYLOAD_TYPE}","payload":"{}","signatures":[{{"sig":"{}"}}]}}"#,
            BASE64.encode(&payload),
            BASE64.encode(&sig)
        );
        let out = verify_dsse_envelope_offline(&leaf, env_absent.as_bytes(), ARTIFACT_SHA256);
        assert_eq!(out.status, CheckStatus::Verified, "{}", out.reason);
    }
}
