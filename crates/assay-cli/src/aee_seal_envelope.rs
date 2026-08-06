//! ADR-045 production signing envelope for the Landlock run-end seal.
//!
//! `aee_seal` assembles a payload and stops short of signing, because ADR-045 authorises "only the
//! primitive design and fixture/checker work, not stable production AEE export" until the signing
//! surface is chosen and tested. This module is that choice, and it closes the first of the ADR's
//! five production gates: *Landlock/TCP-connect can emit a signed run-end sealed record.*
//!
//! # What is chosen, and why
//!
//! **DSSE, named explicitly**, which is the option the ADR reserves ("with any DSSE use called out
//! by name if selected"). Three reasons, in order of weight:
//!
//! 1. `assay_common::dsse::build_pae` is already the workspace's one Pre-Authentication Encoding,
//!    shared by `assay-evidence`, `assay-core` and `assay-registry`. PAE defines what a signature
//!    covers, so a second construction of it would be a second definition of what a signature
//!    means. This calls the shared one.
//! 2. AEE (in-toto/attestation#570) is a DSSE-carried statement, so a seal that a consumer can
//!    lift into an AEE statement without re-signing is worth more than a bespoke envelope.
//! 3. DSSE carries multiple signatures over one payload, which is what later per-collection-path
//!    keys will need. The ADR already requires the payload to carry collection-path identity for
//!    exactly that reason.
//!
//! **RFC 8785 canonical JSON, even though DSSE does not need it.** DSSE's PAE binds the payload
//! bytes as transmitted, so canonicalization is not required for the signature to be stable. It is
//! required anyway, for a different job: the checker *recomputes* `aeeRunBinding` and
//! `aeeObservedSet` from the payload, and a recomputation over non-canonical bytes is not
//! reproducible. Signature stability and recomputability are two requirements and PAE only answers
//! the first.
//!
//! **Duplicate object members rejected before signing and before verification.** `parse_strict`
//! fails closed on a duplicate key at any depth. `serde_json` would silently keep the last value,
//! so a signature could cover bytes whose meaning a reader resolves differently.
//!
//! **Ed25519.** Asymmetric, as the ADR requires, and already a workspace dependency. The fixture
//! harness signs with HMAC over the same PAE; that key shape cannot verify here, which is the
//! ADR's "fixture-only signing cannot be mistaken for production observation signing" made
//! structural rather than documentary.
//!
//! # The ordering that matters
//!
//! ADR-045: *"verify the envelope signature and key scope over the exact signed bytes before
//! decoding the payload."* [`verify_seal`] returns the payload only after both checks pass, and it
//! never hands back a parsed value on any path where either failed. A verifier that decoded first
//! would give a caller fields to read from an envelope nobody had authenticated -- which is the
//! whole failure mode the seal exists to remove.

use assay_canonical::{jcs, parse_strict};
use assay_common::dsse::build_pae;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::aee_seal::SealPayload;

/// The producer-owned media type for a production seal payload.
///
/// Distinct from the fixture harness's `…fixture.v0+json` by construction, so a fixture envelope
/// cannot be replayed into a production verifier: the PAE binds the payload type, so the two sign
/// different bytes even for identical payloads.
pub const PAYLOAD_TYPE: &str = "application/vnd.assay.aee-landlock-seal.v1+json";

/// The key role permitted to sign a substrate observation.
///
/// ADR-045: "The production seal signer is a substrate observation key role, not a policy-decision
/// key," and "policy-decision keys MUST NOT sign substrate observation records." A role carried as
/// a typed value rather than a string means the wrong role cannot be passed by spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyRole {
    SubstrateObservation,
    PolicyDecision,
}

/// What consumer policy has decided about one key.
///
/// ADR-045 requires the trusted scope to bind at least the substrate identity, the collection path
/// and the key role. All three are here and all three are checked; a key trusted for something else
/// is structurally valid and not credited.
#[derive(Debug, Clone)]
pub struct TrustedObservationKey {
    pub keyid: String,
    pub role: KeyRole,
    pub verifying_key: VerifyingKey,
    pub collection_paths: Vec<String>,
    pub substrate: String,
}

/// A DSSE envelope over a canonical seal payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SealEnvelope {
    /// Base64 of the RFC 8785 canonical payload bytes.
    pub payload: String,
    #[serde(rename = "payloadType")]
    pub payload_type: String,
    pub signatures: Vec<EnvelopeSignature>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvelopeSignature {
    pub keyid: String,
    pub sig: String,
}

/// Why an envelope was refused.
///
/// Every variant is a refusal, and there is deliberately no "could not check" that a caller might
/// read as success. A seal that cannot be verified is not a seal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SealVerifyError {
    /// The envelope's payload type is not this producer's. Includes a fixture envelope.
    UnsupportedPayloadType(String),
    /// The envelope does not carry exactly one signature. Multi-signature is a later slice, and
    /// accepting a set this verifier cannot reason about would credit the wrong one.
    SignatureCount(usize),
    PayloadNotBase64,
    /// Duplicate object member, or not JSON at all. Rejected before the signature is trusted for
    /// anything, and before any field is read.
    PayloadNotStrictJson(String),
    SignatureNotBase64,
    SignatureMalformed,
    /// The signature does not verify over the PAE of the exact transmitted bytes.
    SignatureInvalid,
    /// The signing key is not the key consumer policy names.
    KeyNotTrusted { keyid: String },
    /// A policy-decision key signed a substrate observation.
    WrongKeyRole,
    /// The key is trusted, but not for the collection path this payload claims.
    CollectionPathOutOfScope { path: String },
    /// The key is trusted, but not for this substrate.
    SubstrateOutOfScope { substrate: String },
    /// The bytes verified, but they do not deserialize into a seal payload.
    PayloadNotASeal(String),
}

impl SealVerifyError {
    /// A stable code, so a consumer can branch on the refusal without parsing prose.
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnsupportedPayloadType(_) => "seal-envelope-unsupported-payload-type",
            Self::SignatureCount(_) => "seal-envelope-signature-count",
            Self::PayloadNotBase64 => "seal-envelope-payload-not-base64",
            Self::PayloadNotStrictJson(_) => "seal-envelope-payload-not-strict-json",
            Self::SignatureNotBase64 => "seal-envelope-signature-not-base64",
            Self::SignatureMalformed => "seal-envelope-signature-malformed",
            Self::SignatureInvalid => "seal-envelope-signature-invalid",
            Self::KeyNotTrusted { .. } => "seal-envelope-key-not-trusted",
            Self::WrongKeyRole => "seal-envelope-wrong-key-role",
            Self::CollectionPathOutOfScope { .. } => "seal-envelope-collection-path-out-of-scope",
            Self::SubstrateOutOfScope { .. } => "seal-envelope-substrate-out-of-scope",
            Self::PayloadNotASeal(_) => "seal-envelope-payload-not-a-seal",
        }
    }
}

/// Why a payload could not be signed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SealSignError {
    /// The payload does not canonicalize. Refused rather than signed in some other form.
    NotCanonicalizable(String),
    /// The signer is not a substrate observation key.
    WrongKeyRole,
}

/// The exact bytes a seal signature covers: RFC 8785 canonical JSON, UTF-8.
///
/// One function, called by both [`sign_seal`] and [`verify_seal`], so the producer and the verifier
/// cannot disagree about what was signed.
fn canonical_payload_bytes(payload: &SealPayload) -> Result<Vec<u8>, SealSignError> {
    jcs::to_vec(payload).map_err(|e| SealSignError::NotCanonicalizable(e.to_string()))
}

/// Sign a seal payload with a substrate observation key.
pub fn sign_seal(
    payload: &SealPayload,
    signing_key: &SigningKey,
    keyid: &str,
    role: KeyRole,
) -> Result<SealEnvelope, SealSignError> {
    if role != KeyRole::SubstrateObservation {
        return Err(SealSignError::WrongKeyRole);
    }
    let bytes = canonical_payload_bytes(payload)?;
    let signature = signing_key.sign(&build_pae(PAYLOAD_TYPE, &bytes));
    Ok(SealEnvelope {
        payload: BASE64.encode(&bytes),
        payload_type: PAYLOAD_TYPE.to_string(),
        signatures: vec![EnvelopeSignature {
            keyid: keyid.to_string(),
            sig: BASE64.encode(signature.to_bytes()),
        }],
    })
}

/// Verify an envelope and its key's scope, then return the payload.
///
/// The order is the contract. Nothing about the payload is returned until the signature verifies
/// over the exact transmitted bytes *and* consumer policy credits the key for this payload's
/// collection path and substrate. `collection_path` is read from the strictly-parsed bytes before
/// the scope check, which is unavoidable -- the scope check is about that value -- but the parse
/// only ever produces a value used to *refuse*, never one handed to a caller.
pub fn verify_seal(
    envelope: &SealEnvelope,
    trusted: &TrustedObservationKey,
) -> Result<SealPayload, SealVerifyError> {
    if envelope.payload_type != PAYLOAD_TYPE {
        return Err(SealVerifyError::UnsupportedPayloadType(
            envelope.payload_type.clone(),
        ));
    }
    let entry = match envelope.signatures.as_slice() {
        [only] => only,
        other => return Err(SealVerifyError::SignatureCount(other.len())),
    };

    let bytes = BASE64
        .decode(envelope.payload.as_bytes())
        .map_err(|_| SealVerifyError::PayloadNotBase64)?;

    // Strict parse before anything trusts these bytes. A duplicate member would let a signature
    // cover one reading while a consumer resolves another, and RFC 8785 treats it as invalid.
    let value = parse_strict(std::str::from_utf8(&bytes).map_err(|e| {
        SealVerifyError::PayloadNotStrictJson(e.to_string())
    })?)
    .map_err(|e| SealVerifyError::PayloadNotStrictJson(e.to_string()))?;

    if entry.keyid != trusted.keyid {
        return Err(SealVerifyError::KeyNotTrusted {
            keyid: entry.keyid.clone(),
        });
    }
    if trusted.role != KeyRole::SubstrateObservation {
        return Err(SealVerifyError::WrongKeyRole);
    }

    let sig_bytes = BASE64
        .decode(entry.sig.as_bytes())
        .map_err(|_| SealVerifyError::SignatureNotBase64)?;
    let sig_array: [u8; 64] = sig_bytes
        .as_slice()
        .try_into()
        .map_err(|_| SealVerifyError::SignatureMalformed)?;
    let signature = Signature::from_bytes(&sig_array);

    // Over the PAE of the exact transmitted bytes, not of a re-serialization of the parsed value.
    // Re-serializing would verify a signature over bytes the sender never sent.
    trusted
        .verifying_key
        .verify(&build_pae(PAYLOAD_TYPE, &bytes), &signature)
        .map_err(|_| SealVerifyError::SignatureInvalid)?;

    let path = value
        .get("assayCollectionPath")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    if !trusted.collection_paths.iter().any(|p| *p == path) {
        return Err(SealVerifyError::CollectionPathOutOfScope { path });
    }

    let payload: SealPayload = serde_json::from_value(value)
        .map_err(|e| SealVerifyError::PayloadNotASeal(e.to_string()))?;
    Ok(payload)
}

/// Whether a key's trusted scope covers this statement's substrate.
///
/// Separate from [`verify_seal`] because the substrate identity comes from the assembled statement
/// rather than the payload, and a function that took both would invite a caller to pass the
/// statement's own claim as the thing to check it against.
pub fn check_substrate_scope(
    trusted: &TrustedObservationKey,
    statement_substrate: &str,
) -> Result<(), SealVerifyError> {
    if trusted.substrate != statement_substrate {
        return Err(SealVerifyError::SubstrateOutOfScope {
            substrate: statement_substrate.to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aee_seal::{build_sealed_run, DropAccounting, ObservationEnvironment};
    use crate::enforcement_health_v1::{EnforcementHealthV1, Probe};

    const PARITY: &str = include_str!(
        "../../../scripts/experiments/fixtures/aee-landlock-seal/derivation-parity.json"
    );

    fn env_from_parity() -> ObservationEnvironment {
        let p: serde_json::Value = serde_json::from_str(PARITY).expect("parity vectors parse");
        let e = &p["environment"];
        let g = |k: &str| e[k].as_str().expect("digest").to_string();
        ObservationEnvironment {
            subject_digest: g("subject"),
            substrate_digest: g("substrate"),
            corpus_digest: g("corpus"),
            catch_policy_digest: g("catchPolicy"),
            observation_vocabulary_digest: g("observationVocabulary"),
            run_entropy_digest: g("runEntropy"),
            network_posture: e["networkPosture"].clone(),
        }
    }

    fn sealed_payload() -> SealPayload {
        let health = EnforcementHealthV1::landlock_active(
            4,
            vec![443],
            Some(Probe {
                kind: "real_block".into(),
                transport: "ipv4".into(),
                blocked_action: "tcp_connect".into(),
                blocked_port: 4444,
                blocked_errno: "EACCES".into(),
                listener_reached: false,
            }),
        );
        build_sealed_run(
            &health,
            &env_from_parity(),
            &[],
            "2026-08-05T00:00:00Z",
            &DropAccounting::SynchronousProbe,
        )
        .expect("seal-eligible")
        .seal
    }

    fn key() -> SigningKey {
        SigningKey::from_bytes(&[7u8; 32])
    }

    fn trusted(k: &SigningKey) -> TrustedObservationKey {
        TrustedObservationKey {
            keyid: "observer-1".into(),
            role: KeyRole::SubstrateObservation,
            verifying_key: k.verifying_key(),
            collection_paths: vec![crate::aee_seal::COLLECTION_PATH_LANDLOCK_TCP_CONNECT.into()],
            substrate: "assay-landlock-substrate".into(),
        }
    }

    fn signed() -> (SealEnvelope, SigningKey) {
        let k = key();
        let env = sign_seal(&sealed_payload(), &k, "observer-1", KeyRole::SubstrateObservation)
            .expect("signs");
        (env, k)
    }

    #[test]
    fn a_signed_seal_round_trips() {
        let (envelope, k) = signed();
        let payload = verify_seal(&envelope, &trusted(&k)).expect("verifies");
        assert_eq!(payload, sealed_payload());
    }

    /// The signature covers the exact transmitted bytes, so the same payload re-encoded a different
    /// way must not verify. This is what makes the envelope bind bytes rather than meaning.
    ///
    /// Pretty-printing rather than reordering: `serde_json`'s map is a `BTreeMap`, so a round-trip
    /// through `Value` already comes back in JCS key order and produces byte-identical output. The
    /// first version of this test reordered and passed for that reason -- it was asserting nothing.
    /// Whitespace is a difference RFC 8785 forbids and JSON semantics ignore, which is exactly the
    /// gap between "same meaning" and "same bytes".
    #[test]
    fn the_same_payload_encoded_differently_does_not_verify() {
        let (mut envelope, k) = signed();
        let bytes = BASE64.decode(envelope.payload.as_bytes()).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let pretty = serde_json::to_vec_pretty(&value).unwrap();
        assert_ne!(pretty, bytes, "the re-encoding must actually differ");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&pretty).unwrap(),
            value,
            "and must still mean the same thing"
        );
        envelope.payload = BASE64.encode(&pretty);
        let err = verify_seal(&envelope, &trusted(&k)).expect_err("must refuse");
        assert_eq!(err.code(), "seal-envelope-signature-invalid");
    }

    #[test]
    fn a_tampered_payload_does_not_verify() {
        let (mut envelope, k) = signed();
        let bytes = BASE64.decode(envelope.payload.as_bytes()).unwrap();
        let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        value["aeeStillArmed"] = serde_json::json!(false);
        envelope.payload = BASE64.encode(jcs::to_vec(&value).unwrap());
        let err = verify_seal(&envelope, &trusted(&k)).expect_err("must refuse");
        assert_eq!(err.code(), "seal-envelope-signature-invalid");
    }

    /// A duplicate object member is refused before the signature is trusted for anything.
    ///
    /// `serde_json` keeps the last value silently, so a signature could cover one reading while a
    /// consumer resolves another. RFC 8785 treats it as invalid and so does this.
    #[test]
    fn a_duplicate_member_is_refused() {
        let (mut envelope, k) = signed();
        let bytes = BASE64.decode(envelope.payload.as_bytes()).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        let doubled = text.replacen('{', r#"{"aeeStillArmed":false,"#, 1);
        envelope.payload = BASE64.encode(doubled.as_bytes());
        let err = verify_seal(&envelope, &trusted(&k)).expect_err("must refuse");
        assert_eq!(err.code(), "seal-envelope-payload-not-strict-json");
    }

    /// The ADR's "fixture-only signing cannot be mistaken for production" gate, made structural.
    ///
    /// The PAE binds the payload type, so a fixture envelope signs different bytes than a
    /// production one even for an identical payload -- and this verifier refuses the type outright,
    /// before reaching the signature at all.
    #[test]
    fn a_fixture_envelope_is_refused_by_type() {
        let (mut envelope, k) = signed();
        envelope.payload_type = "application/vnd.assay.aee-landlock-seal.fixture.v0+json".into();
        let err = verify_seal(&envelope, &trusted(&k)).expect_err("must refuse");
        assert_eq!(err.code(), "seal-envelope-unsupported-payload-type");
    }

    #[test]
    fn a_policy_decision_key_may_not_sign() {
        let err = sign_seal(&sealed_payload(), &key(), "policy-1", KeyRole::PolicyDecision)
            .expect_err("must refuse");
        assert_eq!(err, SealSignError::WrongKeyRole);
    }

    #[test]
    fn a_policy_decision_key_may_not_be_credited() {
        let (envelope, k) = signed();
        let mut t = trusted(&k);
        t.role = KeyRole::PolicyDecision;
        let err = verify_seal(&envelope, &t).expect_err("must refuse");
        assert_eq!(err.code(), "seal-envelope-wrong-key-role");
    }

    #[test]
    fn an_untrusted_key_is_not_credited() {
        let (envelope, k) = signed();
        let mut t = trusted(&k);
        t.keyid = "someone-else".into();
        let err = verify_seal(&envelope, &t).expect_err("must refuse");
        assert_eq!(err.code(), "seal-envelope-key-not-trusted");
    }

    /// A valid signature from a key trusted for a different collection path is structurally valid
    /// and not credited, which is the ADR's scope rule.
    #[test]
    fn a_key_outside_its_collection_path_is_not_credited() {
        let (envelope, k) = signed();
        let mut t = trusted(&k);
        t.collection_paths = vec!["landlock-udp-send".into()];
        let err = verify_seal(&envelope, &t).expect_err("must refuse");
        assert_eq!(err.code(), "seal-envelope-collection-path-out-of-scope");
    }

    #[test]
    fn a_key_outside_its_substrate_is_not_credited() {
        let k = key();
        let err = check_substrate_scope(&trusted(&k), "some-other-substrate")
            .expect_err("must refuse");
        assert_eq!(err.code(), "seal-envelope-substrate-out-of-scope");
        check_substrate_scope(&trusted(&k), "assay-landlock-substrate").expect("in scope");
    }

    /// Multi-signature is a later slice. Accepting a set this verifier cannot reason about would
    /// mean crediting whichever entry it happened to look at.
    #[test]
    fn an_envelope_without_exactly_one_signature_is_refused() {
        let (mut envelope, k) = signed();
        let only = envelope.signatures[0].clone();
        envelope.signatures = vec![only.clone(), only];
        assert_eq!(
            verify_seal(&envelope, &trusted(&k)).expect_err("must refuse").code(),
            "seal-envelope-signature-count"
        );
        envelope.signatures.clear();
        assert_eq!(
            verify_seal(&envelope, &trusted(&k)).expect_err("must refuse").code(),
            "seal-envelope-signature-count"
        );
    }

    /// Producer and verifier agree on what was signed because one function produces the bytes.
    #[test]
    fn the_signed_bytes_are_rfc8785_canonical() {
        let payload = sealed_payload();
        let bytes = canonical_payload_bytes(&payload).expect("canonicalizes");
        assert_eq!(bytes, jcs::to_vec(&payload).unwrap());
        // And the envelope carries exactly those bytes, not a re-serialization.
        let (envelope, _) = signed();
        assert_eq!(BASE64.decode(envelope.payload.as_bytes()).unwrap(), bytes);
    }

    /// No refusal path returns a payload. A caller that ignored the error type still cannot read
    /// fields from an envelope nobody authenticated.
    #[test]
    fn no_refusal_returns_a_payload() {
        let (envelope, k) = signed();
        let mut wrong_key = trusted(&k);
        wrong_key.verifying_key = SigningKey::from_bytes(&[9u8; 32]).verifying_key();
        assert!(verify_seal(&envelope, &wrong_key).is_err());

        let mut wrong_type = envelope.clone();
        wrong_type.payload_type = "text/plain".into();
        assert!(verify_seal(&wrong_type, &trusted(&k)).is_err());

        let mut bad_b64 = envelope.clone();
        bad_b64.payload = "!!!".into();
        assert!(verify_seal(&bad_b64, &trusted(&k)).is_err());
    }
}
