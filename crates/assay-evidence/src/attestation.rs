//! In-toto / DSSE attestation over an evidence bundle manifest (ADR-039).
//!
//! Wraps a bundle [`Manifest`] as an in-toto v1 Statement and signs it as a DSSE
//! envelope, reusing the mandate DSSE primitives (PAE + Ed25519). The anchor
//! (a transparency log or timestamp) stays pluggable and external.
//!
//! Honest boundary: an attestation binds who-said-it and the *semantic event chain*.
//! It does NOT upgrade observed support, and provides no trust root or transparency
//! log on its own.
//!
//! It also does not identify the artifact. The subject digest is `manifest.run_root`,
//! a chain over per-event content hashes, and those cover exactly
//! `{specversion, type, datacontenttype, subject?, data}` so a re-export stays stable.
//! Everything else is outside by construction, including stream identity, `time`,
//! trace context, producer and policy metadata, and the privacy flags; the enumerated
//! list lives in `crypto/id.rs`. A bundle
//! whose run id, event ids, producer, timestamps and PII flags are rewritten
//! consistently therefore has the same `run_root` and satisfies the same attestation.
//! in-toto expects the reverse -- subjects are immutable and matched purely by digest
//! -- so treating a satisfied envelope as proof of *which* bundle you hold reads a
//! guarantee into the subject that it does not carry. See ADR-039 "Non-claims".

use crate::bundle::writer::VerifyResult;
use crate::crypto::jcs;
use crate::mandate::signing::{build_pae, compute_key_id_from_verifying_key};
use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ed25519_dalek::{Signature as Ed25519Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// in-toto Statement type URI (v1).
const STATEMENT_TYPE: &str = "https://in-toto.io/Statement/v1";
/// DSSE payload type for in-toto statements.
const IN_TOTO_PAYLOAD_TYPE: &str = "application/vnd.in-toto+json";
/// Assay evidence-bundle predicate type (v0; not a frozen public spec).
pub const EVIDENCE_BUNDLE_PREDICATE_TYPE: &str = "https://assay.dev/attestation/evidence-bundle/v1";

/// The predicate this crate emitted before ADR-044.
///
/// Kept only so a consumer can name what it is refusing. A v0 statement puts `run_root` in the
/// subject digest, which identifies no artifact, and carries an unconstrained predicate. It is not
/// upgradable: nothing in it says which bytes it described.
pub const EVIDENCE_BUNDLE_PREDICATE_TYPE_V0: &str =
    "https://assay.dev/attestation/evidence-bundle/v0";

/// in-toto subject: a named artifact plus its content digest(s).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Subject {
    pub name: String,
    pub digest: BTreeMap<String, String>,
}

/// in-toto v1 Statement.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InTotoStatement {
    #[serde(rename = "_type")]
    pub type_: String,
    pub subject: Vec<Subject>,
    #[serde(rename = "predicateType")]
    pub predicate_type: String,
    pub predicate: serde_json::Value,
}

/// Build the in-toto Statement for a bundle, from the bytes a consumer will hold.
///
/// The subject digest is the SHA-256 of the completed archive — gzip trailer included — because
/// in-toto matches subjects purely by digest and requires them to identify an immutable artifact.
/// The previous version put `manifest.run_root` there, which is a semantic-equivalence digest: two
/// bundles differing in `run_id`, producer, timestamps and the PII flag share it, so it named no
/// artifact and, being nobody's archive hash, matched none either. ADR-044.
///
/// Takes the bytes rather than a `Manifest` for that reason: a manifest cannot produce a
/// conforming subject, so there is no signature that could accept one by mistake.
pub fn statement_from_bundle(
    bundle_bytes: &[u8],
    predicate: &EvidenceBundlePredicate,
) -> Result<InTotoStatement> {
    let artifact = hex::encode(Sha256::digest(bundle_bytes));
    let mut digest = BTreeMap::new();
    // Exactly one entry. `DigestSet`s match when ANY field matches, so a second key — a
    // `run_root` added to document the semantic root, say — would match a forged bundle through
    // the very field meant to explain the problem. The semantic root lives in the predicate.
    digest.insert("sha256".to_string(), artifact);
    Ok(InTotoStatement {
        type_: STATEMENT_TYPE.to_string(),
        subject: vec![Subject {
            name: predicate.run.run_id.clone(),
            digest,
        }],
        predicate_type: EVIDENCE_BUNDLE_PREDICATE_TYPE.to_string(),
        predicate: serde_json::to_value(predicate).context("serialize predicate")?,
    })
}

/// Derive the v1 predicate from a verified bundle.
///
/// Every field comes from the verification result, so the predicate cannot describe a bundle other
/// than the one that was checked.
pub fn predicate_from_verified(result: &VerifyResult) -> Result<EvidenceBundlePredicate> {
    let (start, end) = result
        .time_window
        .clone()
        .context("a verified bundle has at least one event, so it has a time window")?;
    Ok(EvidenceBundlePredicate {
        schema_version: 1,
        semantic_equivalence: SemanticEquivalence {
            algorithm: "assay-run-root-v1".to_string(),
            value: result.computed_run_root.clone(),
        },
        run: PredicateRun {
            run_id: result.manifest.run_id.clone(),
            event_count: result.event_count,
            producer: PredicateProducer {
                name: result.manifest.producer.name.clone(),
                version: result.manifest.producer.version.clone(),
                git: result.manifest.producer.git.clone().unwrap_or_default(),
            },
            time_window: TimeWindow { start, end },
        },
    })
}

/// `run_root` under the algorithm that produced it.
///
/// A named block, not a bare field, so a consumer cannot read the semantic digest as a second
/// artifact digest — ADR-044 decision 3. Matching semantics do not apply inside a predicate, which
/// is exactly why it belongs here and not in the subject's `DigestSet`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticEquivalence {
    pub algorithm: String,
    pub value: String,
}

/// Who produced the run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PredicateProducer {
    pub name: String,
    pub version: String,
    pub git: String,
}

/// Earliest and latest event `time`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimeWindow {
    pub start: String,
    pub end: String,
}

/// The run this attestation describes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PredicateRun {
    pub run_id: String,
    pub event_count: usize,
    pub producer: PredicateProducer,
    /// Required, with no null case: ADR-044 dropped that when the verifier stopped accepting a
    /// zero-event bundle, so there is always a first and a last event time.
    pub time_window: TimeWindow,
}

/// The `evidence-bundle/v1` predicate.
///
/// Every field is derivable from the bundle the subject names, which is what makes
/// [`verify_attestation_for_bundle`] able to cross-check rather than trust.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidenceBundlePredicate {
    pub schema_version: u32,
    pub semantic_equivalence: SemanticEquivalence,
    pub run: PredicateRun,
}

/// The outcome of checking a DSSE envelope without any artifact to match it against.
///
/// A distinct state rather than a success or an error. `verify_envelope` establishes who signed
/// what; it cannot establish which artifact, because it never sees one. Returning a plain
/// `Statement` let a caller read "verified" into a result that had matched nothing — the state
/// this ADR was written to make nameable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureVerified {
    pub statement: InTotoStatement,
}

/// A fully verified attestation: signature checked AND subject matched against real bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttestationVerified {
    pub statement: InTotoStatement,
    pub predicate: EvidenceBundlePredicate,
    /// The digest that matched, as hex without the algorithm prefix.
    pub artifact_sha256: String,
}

/// A single DSSE signature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DsseSignature {
    pub keyid: String,
    pub sig: String,
}

/// A DSSE envelope carrying an in-toto attestation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DsseEnvelope {
    pub payload: String,
    #[serde(rename = "payloadType")]
    pub payload_type: String,
    pub signatures: Vec<DsseSignature>,
}

/// Sign an in-toto Statement as a DSSE envelope (Ed25519 over the DSSE PAE of the
/// JCS-canonicalized statement).
pub fn sign_statement(statement: &InTotoStatement, key: &SigningKey) -> Result<DsseEnvelope> {
    let canonical = jcs::to_vec(statement).context("canonicalize in-toto statement")?;
    let pae = build_pae(IN_TOTO_PAYLOAD_TYPE, &canonical);
    let signature: Ed25519Signature = key.sign(&pae);
    let keyid = compute_key_id_from_verifying_key(&key.verifying_key())?;
    Ok(DsseEnvelope {
        payload: BASE64.encode(&canonical),
        payload_type: IN_TOTO_PAYLOAD_TYPE.to_string(),
        signatures: vec![DsseSignature {
            keyid,
            sig: BASE64.encode(signature.to_bytes()),
        }],
    })
}

/// Check the signature and the Statement shape. Nothing more.
///
/// The result is [`SignatureVerified`], a state and not a success: it establishes who signed a
/// Statement, not which artifact the Statement describes, because no artifact is offered here.
/// Callers holding the bundle bytes want [`verify_attestation_for_bundle`]; callers that do not
/// must not report an attestation as verified. ADR-044 decision 2 keeps these two calls apart so
/// the weaker outcome has a name — folding the byte match in here would make "I verified the
/// attestation" mean different things depending on which argument the caller happened to have.
pub fn verify_envelope(
    envelope: &DsseEnvelope,
    trusted_key: &VerifyingKey,
) -> Result<SignatureVerified> {
    // Reject any DSSE payload type other than in-toto BEFORE verifying, so a key
    // that signed the same bytes under a different payload type cannot be accepted
    // as an in-toto attestation (payload-type confusion). The PAE binds the type,
    // so we must verify under the type we require, not the one the envelope claims.
    if envelope.payload_type != IN_TOTO_PAYLOAD_TYPE {
        anyhow::bail!(
            "unexpected DSSE payloadType: expected {}, got {}",
            IN_TOTO_PAYLOAD_TYPE,
            envelope.payload_type
        );
    }
    let canonical = BASE64
        .decode(&envelope.payload)
        .context("decode dsse payload")?;
    let pae = build_pae(IN_TOTO_PAYLOAD_TYPE, &canonical);
    let dsse_sig = envelope
        .signatures
        .first()
        .context("dsse envelope has no signatures")?;
    let sig_bytes = BASE64.decode(&dsse_sig.sig).context("decode signature")?;
    let sig_array: [u8; 64] = sig_bytes
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("signature is not 64 bytes"))?;
    let signature = Ed25519Signature::from_bytes(&sig_array);
    trusted_key
        .verify(&pae, &signature)
        .context("dsse signature verification failed")?;
    let statement: InTotoStatement =
        serde_json::from_slice(&canonical).context("parse in-toto statement")?;
    // Defense in depth: the verified payload must be a v1 in-toto Statement.
    if statement.type_ != STATEMENT_TYPE {
        anyhow::bail!(
            "unexpected in-toto statement _type: expected {}, got {}",
            STATEMENT_TYPE,
            statement.type_
        );
    }
    Ok(SignatureVerified { statement })
}

/// Verify an attestation against the bundle bytes it claims to describe.
///
/// This is the only call that can return a fully verified attestation, because it is the only one
/// that sees an artifact. It recomputes the SHA-256 of `bundle_bytes`, matches it against the
/// single subject, requires the v1 predicate, and cross-checks every field the bundle can produce.
///
/// A correct subject digest nobody compares is still an unchecked field: before ADR-044 the
/// verifier read `subject` nowhere at all, which is why a forged bundle survived — not because it
/// matched, but because nothing was matched.
pub fn verify_attestation_for_bundle(
    envelope: &DsseEnvelope,
    trusted_key: &VerifyingKey,
    bundle_bytes: &[u8],
) -> Result<AttestationVerified> {
    let verified = verify_envelope(envelope, trusted_key)?;
    let statement = verified.statement;

    if statement.predicate_type == EVIDENCE_BUNDLE_PREDICATE_TYPE_V0 {
        anyhow::bail!(
            "attestation uses the pre-ADR-044 predicate {EVIDENCE_BUNDLE_PREDICATE_TYPE_V0}, \
             whose subject digest identifies no artifact; it cannot be matched and must be reissued"
        );
    }
    if statement.predicate_type != EVIDENCE_BUNDLE_PREDICATE_TYPE {
        anyhow::bail!(
            "unknown predicate type {}: expected {}. An unrecognised major version fails closed \
             rather than being reported as verified",
            statement.predicate_type,
            EVIDENCE_BUNDLE_PREDICATE_TYPE
        );
    }

    // Exactly one subject, and it identifies an artifact. Broader than barring `run_root` from the
    // DigestSet: the spec requires every entry to identify an immutable artifact, so barring one
    // field would only invite the next.
    if statement.subject.len() != 1 {
        anyhow::bail!(
            "expected exactly one subject, found {}",
            statement.subject.len()
        );
    }
    let subject = &statement.subject[0];
    if subject.digest.len() != 1 {
        anyhow::bail!(
            "subject carries {} digest entries; DigestSets match on ANY field, so a second entry \
             would let a different artifact match",
            subject.digest.len()
        );
    }
    let claimed = subject
        .digest
        .get("sha256")
        .context("subject digest has no sha256 entry")?;

    let actual = hex::encode(Sha256::digest(bundle_bytes));
    if claimed != &actual {
        anyhow::bail!("subject digest {claimed} does not match the bundle's sha256 {actual}");
    }

    let predicate: EvidenceBundlePredicate =
        serde_json::from_value(statement.predicate.clone()).context("parse v1 predicate")?;
    if predicate.schema_version != 1 {
        anyhow::bail!(
            "predicate schema_version {} is not 1",
            predicate.schema_version
        );
    }

    // Cross-check against the artifact rather than trusting the predicate. A predicate that
    // disagrees with the bundle it is attached to is a rejection, not a note.
    let result = crate::bundle::verify_bundle(bundle_bytes)
        .context("the attested bundle does not verify")?;
    let derived = predicate_from_verified(&result)?;
    if derived != predicate {
        anyhow::bail!(
            "predicate disagrees with the bundle it is attached to: attested {predicate:?}, \
             derived {derived:?}"
        );
    }

    Ok(AttestationVerified {
        statement,
        predicate,
        artifact_sha256: actual,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_statement() -> InTotoStatement {
        let mut digest = BTreeMap::new();
        digest.insert("sha256".to_string(), "abc123".to_string());
        InTotoStatement {
            type_: STATEMENT_TYPE.to_string(),
            subject: vec![Subject {
                name: "bundle-1".to_string(),
                digest,
            }],
            predicate_type: EVIDENCE_BUNDLE_PREDICATE_TYPE.to_string(),
            predicate: serde_json::json!({ "event_count": 3, "outcome": "supported" }),
        }
    }

    #[test]
    fn sign_then_verify_roundtrips_and_detects_tamper() {
        let key = SigningKey::from_bytes(&[7u8; 32]);
        let statement = sample_statement();

        let envelope = sign_statement(&statement, &key).expect("sign");
        assert_eq!(envelope.payload_type, IN_TOTO_PAYLOAD_TYPE);

        // A state, not a verdict: the signature is checked, no artifact has been offered.
        let recovered = verify_envelope(&envelope, &key.verifying_key()).expect("verify");
        assert_eq!(recovered.statement, statement);

        // Tampering with the payload must fail verification.
        let mut tampered = envelope.clone();
        let mut bytes = BASE64.decode(&tampered.payload).unwrap();
        bytes[0] ^= 0xFF;
        tampered.payload = BASE64.encode(&bytes);
        assert!(verify_envelope(&tampered, &key.verifying_key()).is_err());

        // A different key must fail verification.
        let other = SigningKey::from_bytes(&[9u8; 32]);
        assert!(verify_envelope(&envelope, &other.verifying_key()).is_err());
    }

    #[test]
    fn verify_rejects_non_in_toto_payload_type() {
        let key = SigningKey::from_bytes(&[7u8; 32]);
        let statement = sample_statement();
        let mut envelope = sign_statement(&statement, &key).expect("sign");

        // Re-label the envelope as a different DSSE payload type. Even with a
        // genuine signature over the same bytes, an in-toto verifier must reject it.
        envelope.payload_type = "application/json".to_string();
        let err = verify_envelope(&envelope, &key.verifying_key())
            .expect_err("must reject non-in-toto payload type");
        assert!(err.to_string().contains("payloadType"));
    }
}
