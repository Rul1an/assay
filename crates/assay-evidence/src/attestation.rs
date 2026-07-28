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
//! The subject identifies the artifact. `subject[0].digest.sha256` is the SHA-256 of the finished
//! `.tar.gz` — the exact byte sequence a consumer receives, gzip trailer included — and
//! `subject[0].name` carries `manifest.bundle_id`. in-toto matches subjects purely by digest and
//! requires them to name an immutable artifact, so this is the only shape a conforming consumer
//! can act on.
//!
//! Before ADR-044 the subject digest was `manifest.run_root`, a chain over per-event content
//! hashes covering only `{specversion, type, datacontenttype, subject?, data}`. Two bundles with
//! different `run_id`, producer, timestamps and PII flags share it, so it named no artifact and,
//! being nobody's archive hash, matched none either. Such statements carry the `v0` predicate type
//! and are refused rather than reinterpreted: nothing in them says which bytes they described.
//!
//! The semantic root still travels, inside the predicate's `semantic_equivalence` block, where
//! `DigestSet` matching semantics do not reach it. See ADR-044.

use crate::bundle::writer::{VerifyLimits, VerifyResult};
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
/// Assay evidence-bundle predicate type (v1).
///
/// On `docs.getassay.dev`, which is where the contract surface actually lives — the profile URIs
/// under `docs/profiles/` already resolve there. The v0 constant below keeps the old host because
/// it names statements that were minted under it; a refusal has to spell the thing being refused.
pub const EVIDENCE_BUNDLE_PREDICATE_TYPE: &str =
    "https://docs.getassay.dev/attestation/evidence-bundle/v1";

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

/// Build and sign-ready the in-toto Statement for a bundle, from the bytes a consumer will hold.
///
/// Takes bytes and nothing else. The predicate and the subject name are both derived from the
/// verification of those same bytes inside this function, so there is no argument a caller could
/// get wrong: the previous shape accepted a caller-constructed predicate alongside the bytes and
/// trusted that they belonged together, which let the public library mint a statement its own
/// consumer would reject. Nothing enforced the pairing except the one call site that happened to
/// do it correctly.
///
/// The subject digest is the SHA-256 of the completed archive — gzip trailer included — because
/// in-toto matches subjects purely by digest and requires them to identify an immutable artifact.
pub fn statement_for_bundle(bundle_bytes: &[u8]) -> Result<InTotoStatement> {
    statement_for_bundle_with_limits(bundle_bytes, VerifyLimits::default())
}

/// [`statement_for_bundle`] under an explicit set of verification ceilings.
pub fn statement_for_bundle_with_limits(
    bundle_bytes: &[u8],
    limits: VerifyLimits,
) -> Result<InTotoStatement> {
    let result = crate::bundle::verify_bundle_with_limits(bundle_bytes, limits)
        .context("the bundle to attest does not verify")?;
    let predicate = predicate_from_verified(&result)?;
    Ok(statement_from_parts(
        bundle_bytes,
        &result.manifest.bundle_id,
        &predicate,
    ))
}

/// Assemble the statement once every part is known to come from the same verified bytes.
///
/// Private on purpose. It is the only place that can pair a subject with a predicate, and the
/// pairing is the thing a caller must not be able to author.
fn statement_from_parts(
    bundle_bytes: &[u8],
    bundle_id: &str,
    predicate: &EvidenceBundlePredicate,
) -> InTotoStatement {
    let artifact = hex::encode(Sha256::digest(bundle_bytes));
    let mut digest = BTreeMap::new();
    // Exactly one entry. `DigestSet`s match when ANY field matches, so a second key — a
    // `run_root` added to document the semantic root, say — would match a forged bundle through
    // the very field meant to explain the problem. The semantic root lives in the predicate.
    digest.insert("sha256".to_string(), artifact);
    InTotoStatement {
        type_: STATEMENT_TYPE.to_string(),
        subject: vec![Subject {
            // ADR-044: the bundle identifier, not `run_id`. `run_id` was written here and never
            // read back, so any name at all satisfied the consumer; a name nobody compares is not
            // an identifier, and this one is now checked against the verified manifest.
            name: bundle_id.to_string(),
            digest,
        }],
        predicate_type: EVIDENCE_BUNDLE_PREDICATE_TYPE.to_string(),
        // Infallible: `EvidenceBundlePredicate` is a plain struct of strings and integers, so
        // `to_value` has no failing case here. It was a `?` returning an error no test could
        // provoke and no caller could act on.
        predicate: serde_json::json!({
            "schema_version": predicate.schema_version,
            "semantic_equivalence": {
                "algorithm": predicate.semantic_equivalence.algorithm,
                "value": predicate.semantic_equivalence.value,
            },
            "run": {
                "run_id": predicate.run.run_id,
                "event_count": predicate.run.event_count,
                "producer": {
                    "name": predicate.run.producer.name,
                    "version": predicate.run.producer.version,
                    "git": predicate.run.producer.git,
                },
                "time_window": {
                    "start": predicate.run.time_window.start,
                    "end": predicate.run.time_window.end,
                },
            },
        }),
    }
}

/// Derive the v1 predicate from a verified bundle.
///
/// Every field comes from the verification result, so the predicate cannot describe a bundle other
/// than the one that was checked.
pub fn predicate_from_verified(result: &VerifyResult) -> Result<EvidenceBundlePredicate> {
    // `AutoSi` keeps whatever sub-second precision the events carried and adds none they did not.
    // `Secs` truncated it, so the predicate claimed a window the bundle never had — and the
    // consumer's exact comparison then held only because it truncated the same way.
    let (start, end) = result.time_window;
    let start = start.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true);
    let end = end.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true);
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

/// Why a signed payload was refused, without quoting any of it.
///
/// The strict parser's own message names the offending key and its path, and both are
/// attacker-chosen strings bounded only by a 1 MB ceiling. A reason is enough to act on.
fn strict_reason(err: &crate::json_strict::StrictJsonError) -> &'static str {
    use crate::json_strict::StrictJsonError as E;
    match err {
        E::DuplicateKey { .. } => "it contains a duplicate object member",
        E::InvalidUnicodeEscape { .. } => "it contains an invalid unicode escape",
        E::LoneSurrogate { .. } => "it contains a lone surrogate",
        E::NestingTooDeep { .. } => "it nests deeper than the parser accepts",
        E::TooManyKeys { .. } => "an object carries more members than the parser accepts",
        E::StringTooLong { .. } => "it contains a string longer than the parser accepts",
        E::ParseError(_) => "it is not well-formed JSON",
    }
}

/// Check the signature and the Statement shape. Nothing more.
///
/// The result is [`SignatureVerified`], a state and not a success: it establishes who signed a
/// Statement, not which artifact the Statement describes, because no artifact is offered here.
/// Callers holding the bundle bytes want [`verify_attestation_for_bundle`]; callers that do not
/// must not report an attestation as verified. ADR-044 decision 2 keeps these two calls apart so
/// the weaker outcome has a name — folding the byte match in here would make "I verified the
/// attestation" mean different things depending on which argument the caller happened to have.
pub fn verify_envelope_signature(
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

    // A valid signature over ambiguous bytes is still ambiguous. `serde_json` resolves duplicate
    // object members last-wins, so a payload carrying `schema_version` twice verified as whichever
    // value happened to come second, and a second conforming consumer reading first-wins would act
    // on the other one — from the same envelope, under the same signature. The strict scanner is
    // the crate's existing answer to that, already applied to manifests and events; the signed
    // payload was the one JSON document reaching a verdict without it.
    let payload = std::str::from_utf8(&canonical).context("signed payload is not UTF-8")?;
    let statement: InTotoStatement = crate::json_strict::from_str_strict(payload).map_err(|e| {
        anyhow::anyhow!(
            "signed payload is not unambiguous JSON: {}",
            strict_reason(&e)
        )
    })?;

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

/// Check the signature and return the Statement.
///
/// Kept at its published 3.x signature. [`verify_envelope_signature`] is the same check returning
/// the named [`SignatureVerified`] state; changing this function's return type instead would have
/// broken every downstream build to add a distinction the new name can carry on its own.
#[deprecated(
    since = "3.36.0",
    note = "use `verify_envelope_signature`, which names the weaker outcome: a checked signature \
            over a Statement is not a verified attestation until an artifact is matched"
)]
pub fn verify_envelope(
    envelope: &DsseEnvelope,
    trusted_key: &VerifyingKey,
) -> Result<InTotoStatement> {
    Ok(verify_envelope_signature(envelope, trusted_key)?.statement)
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
    let verified = verify_envelope_signature(envelope, trusted_key)?;
    let statement = verified.statement;

    if statement.predicate_type == EVIDENCE_BUNDLE_PREDICATE_TYPE_V0 {
        anyhow::bail!(
            "attestation uses the pre-ADR-044 predicate {EVIDENCE_BUNDLE_PREDICATE_TYPE_V0}, \
             whose subject digest identifies no artifact; it cannot be matched and must be reissued"
        );
    }
    if statement.predicate_type != EVIDENCE_BUNDLE_PREDICATE_TYPE {
        anyhow::bail!(
            "unknown predicate type: expected {EVIDENCE_BUNDLE_PREDICATE_TYPE}. An unrecognised \
             major version fails closed rather than being reported as verified"
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

    // Value-free: the holder has both the envelope and the bytes, so echoing the two digests adds
    // nothing they cannot compute and puts an attacker-chosen string into every log that records
    // the failure.
    let actual = hex::encode(Sha256::digest(bundle_bytes));
    if claimed != &actual {
        anyhow::bail!("the subject digest does not match the sha256 of the bundle offered");
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

    // The subject name is `manifest.bundle_id` and is compared, not merely carried. Before this it
    // was written from `run_id` and read nowhere, so every possible name verified — including a
    // name naming a different bundle entirely.
    if subject.name != result.manifest.bundle_id {
        anyhow::bail!("the subject name is not the attested bundle's bundle_id");
    }

    let derived = predicate_from_verified(&result)?;
    if derived != predicate {
        match first_predicate_mismatch(&predicate, &derived) {
            Some(field) => anyhow::bail!(
                "predicate field `{field}` disagrees with the bundle it is attached to"
            ),
            // Unreachable while the namer stays exhaustive, which a compile-time destructure in
            // the tests enforces. Reported rather than ignored: a disagreement the namer cannot
            // place is still a disagreement, and silently accepting it would be the worse failure.
            None => anyhow::bail!(
                "the predicate disagrees with the bundle it is attached to in an unnamed field"
            ),
        }
    }

    Ok(AttestationVerified {
        statement,
        predicate,
        artifact_sha256: actual,
    })
}

/// Name the first field on which two predicates differ, without quoting either value.
///
/// Only ever consulted after an exhaustive `!=`, so it narrows a rejection that has already been
/// decided; it can never widen one. The tests destructure `EvidenceBundlePredicate` exhaustively,
/// so adding a field without extending this function fails to compile.
fn first_predicate_mismatch(
    attested: &EvidenceBundlePredicate,
    derived: &EvidenceBundlePredicate,
) -> Option<&'static str> {
    if attested.schema_version != derived.schema_version {
        return Some("schema_version");
    }
    if attested.semantic_equivalence.algorithm != derived.semantic_equivalence.algorithm {
        return Some("semantic_equivalence.algorithm");
    }
    if attested.semantic_equivalence.value != derived.semantic_equivalence.value {
        return Some("semantic_equivalence.value");
    }
    if attested.run.run_id != derived.run.run_id {
        return Some("run.run_id");
    }
    if attested.run.event_count != derived.run.event_count {
        return Some("run.event_count");
    }
    if attested.run.producer.name != derived.run.producer.name {
        return Some("run.producer.name");
    }
    if attested.run.producer.version != derived.run.producer.version {
        return Some("run.producer.version");
    }
    if attested.run.producer.git != derived.run.producer.git {
        return Some("run.producer.git");
    }
    if attested.run.time_window.start != derived.run.time_window.start {
        return Some("run.time_window.start");
    }
    if attested.run.time_window.end != derived.run.time_window.end {
        return Some("run.time_window.end");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundle::BundleWriter;
    use crate::types::{EvidenceEvent, ProducerMeta};
    use chrono::{DateTime, Utc};

    fn producer() -> ProducerMeta {
        ProducerMeta {
            name: "assay-cli".into(),
            version: "test".into(),
            git: None,
        }
    }

    fn bundle_with_times(times: &[DateTime<Utc>]) -> Vec<u8> {
        let p = producer();
        let mut out = Vec::new();
        let mut w = BundleWriter::new(&mut out).with_producer(p.clone());
        for (i, t) in times.iter().enumerate() {
            w.add_event(
                EvidenceEvent::new(
                    "assay.test.event",
                    "urn:assay:test",
                    "attest_run",
                    i as u64,
                    serde_json::json!({}),
                )
                .with_producer(&p)
                .with_time(*t),
            );
        }
        w.finish().expect("write bundle");
        out
    }

    fn bundle() -> Vec<u8> {
        bundle_with_times(&["2026-07-28T10:00:00Z".parse().unwrap()])
    }

    fn key() -> SigningKey {
        SigningKey::from_bytes(&[7u8; 32])
    }

    /// Sign exactly these payload bytes.
    ///
    /// Every negative case below signs the bytes it wants to test. Mutating an envelope after
    /// signing only ever tests the signature check, which already has its own case — the question
    /// here is what a *validly signed* document is allowed to say.
    fn sign_raw(payload: &str, k: &SigningKey) -> DsseEnvelope {
        let pae = build_pae(IN_TOTO_PAYLOAD_TYPE, payload.as_bytes());
        let sig: Ed25519Signature = k.sign(&pae);
        DsseEnvelope {
            payload: BASE64.encode(payload.as_bytes()),
            payload_type: IN_TOTO_PAYLOAD_TYPE.to_string(),
            signatures: vec![DsseSignature {
                keyid: compute_key_id_from_verifying_key(&k.verifying_key()).unwrap(),
                sig: BASE64.encode(sig.to_bytes()),
            }],
        }
    }

    fn sign_value(v: &serde_json::Value, k: &SigningKey) -> DsseEnvelope {
        sign_raw(&serde_json::to_string(v).unwrap(), k)
    }

    fn statement_value(bytes: &[u8]) -> serde_json::Value {
        serde_json::to_value(statement_for_bundle(bytes).expect("build statement")).unwrap()
    }

    /// Verify a statement built from `bytes` after applying `edit` to its JSON, and return the
    /// rejection message.
    fn reject(bytes: &[u8], edit: impl FnOnce(&mut serde_json::Value)) -> String {
        let k = key();
        let mut v = statement_value(bytes);
        edit(&mut v);
        let envelope = sign_value(&v, &k);
        verify_attestation_for_bundle(&envelope, &k.verifying_key(), bytes)
            .expect_err("must be rejected")
            .to_string()
    }

    // ── the happy path, and the two states it is made of ────────────────────────────────────

    #[test]
    fn a_bundle_attests_and_verifies_against_its_own_bytes() {
        let k = key();
        let bytes = bundle();
        let statement = statement_for_bundle(&bytes).expect("statement");
        let envelope = sign_statement(&statement, &k).expect("sign");

        let signed = verify_envelope_signature(&envelope, &k.verifying_key()).expect("signature");
        assert_eq!(signed.statement, statement);

        let verified = verify_attestation_for_bundle(&envelope, &k.verifying_key(), &bytes)
            .expect("attestation");
        assert_eq!(verified.predicate.run.event_count, 1);
        assert_eq!(
            verified.statement.subject[0].name,
            crate::bundle::verify_bundle(bytes.as_slice())
                .unwrap()
                .manifest
                .bundle_id,
            "the subject names the bundle, not the run"
        );
    }

    #[test]
    fn signature_tampering_and_the_wrong_key_are_refused() {
        let k = key();
        let bytes = bundle();
        let envelope = sign_statement(&statement_for_bundle(&bytes).unwrap(), &k).unwrap();

        let mut tampered = envelope.clone();
        let mut raw = BASE64.decode(&tampered.payload).unwrap();
        raw[0] ^= 0xFF;
        tampered.payload = BASE64.encode(&raw);
        assert!(verify_envelope_signature(&tampered, &k.verifying_key()).is_err());

        let other = SigningKey::from_bytes(&[9u8; 32]);
        assert!(verify_envelope_signature(&envelope, &other.verifying_key()).is_err());
    }

    #[test]
    fn a_non_in_toto_payload_type_is_refused() {
        let k = key();
        let bytes = bundle();
        let mut envelope = sign_statement(&statement_for_bundle(&bytes).unwrap(), &k).unwrap();
        envelope.payload_type = "application/json".to_string();
        let err = verify_envelope_signature(&envelope, &k.verifying_key()).unwrap_err();
        assert!(err.to_string().contains("payloadType"), "got: {err}");
    }

    /// The published 3.x entry point still compiles and still answers.
    ///
    /// The point of the case is the call itself: it is what would stop compiling if the return
    /// type were changed rather than added to.
    #[test]
    #[allow(deprecated)]
    fn the_published_verify_envelope_signature_is_preserved() {
        let k = key();
        let bytes = bundle();
        let envelope = sign_statement(&statement_for_bundle(&bytes).unwrap(), &k).unwrap();

        let statement: InTotoStatement = verify_envelope(&envelope, &k.verifying_key()).unwrap();
        let typed = verify_envelope_signature(&envelope, &k.verifying_key()).unwrap();
        assert_eq!(statement, typed.statement, "the shim must not diverge");
    }

    // ── ambiguous signed JSON, at every level it can occur ──────────────────────────────────

    /// Duplicates are injected into the bytes that get signed, at four nesting levels.
    ///
    /// `serde_json` resolves duplicate members last-wins, so each of these verified before: the
    /// signature was valid and the parser simply picked one of the two claims. Two conforming
    /// consumers with different duplicate policies would then read different attestations out of
    /// one envelope.
    #[test]
    fn duplicate_members_in_the_signed_payload_are_refused_at_every_level() {
        let k = key();
        let bytes = bundle();
        let json = serde_json::to_string(&statement_for_bundle(&bytes).unwrap()).unwrap();

        let cases: [(&str, String, String); 4] = [
            (
                "statement",
                r#""predicateType":"#.to_string(),
                r#""predicateType":"https://example.invalid/other","predicateType":"#.to_string(),
            ),
            (
                "subject/digest",
                r#""sha256":"#.to_string(),
                r#""sha256":"00","sha256":"#.to_string(),
            ),
            (
                "predicate",
                r#""schema_version":1"#.to_string(),
                r#""schema_version":999,"schema_version":1"#.to_string(),
            ),
            (
                "nested predicate",
                r#""event_count":"#.to_string(),
                r#""event_count":999,"event_count":"#.to_string(),
            ),
        ];

        for (level, needle, replacement) in cases {
            let ambiguous = json.replacen(&needle, &replacement, 1);
            assert_ne!(ambiguous, json, "{level}: the injection must apply");

            let envelope = sign_raw(&ambiguous, &k);
            let err = verify_attestation_for_bundle(&envelope, &k.verifying_key(), &bytes)
                .expect_err(&format!("{level}: ambiguous payload must be refused"));
            assert!(
                err.to_string().contains("duplicate object member"),
                "{level}: expected a duplicate-member refusal, got: {err}"
            );
        }
    }

    /// Surplus fields are accepted on purpose: ADR-044 says unknown fields within a known major
    /// are ignored. Pinned so a later `deny_unknown_fields` cannot quietly contradict the decision.
    #[test]
    fn surplus_predicate_fields_are_accepted_within_a_known_major() {
        let k = key();
        let bytes = bundle();
        let mut v = statement_value(&bytes);
        v["predicate"]["a_field_from_a_later_minor"] = serde_json::json!("ignored");
        let envelope = sign_value(&v, &k);
        assert!(verify_attestation_for_bundle(&envelope, &k.verifying_key(), &bytes).is_ok());
    }

    // ── predicate type ─────────────────────────────────────────────────────────────────────

    #[test]
    fn the_v0_predicate_is_refused_by_name() {
        let bytes = bundle();
        let err = reject(&bytes, |v| {
            v["predicateType"] = serde_json::json!(EVIDENCE_BUNDLE_PREDICATE_TYPE_V0);
        });
        assert!(err.contains("pre-ADR-044"), "got: {err}");
    }

    #[test]
    fn an_unknown_predicate_major_fails_closed() {
        let bytes = bundle();
        let err = reject(&bytes, |v| {
            v["predicateType"] =
                serde_json::json!("https://docs.getassay.dev/attestation/evidence-bundle/v2");
        });
        assert!(err.contains("unknown predicate type"), "got: {err}");
    }

    // ── subject shape ──────────────────────────────────────────────────────────────────────

    #[test]
    fn the_subject_array_must_hold_exactly_one_entry() {
        let bytes = bundle();
        for (name, edit) in [("zero", 0usize), ("two", 2usize)] {
            let err = reject(&bytes, |v| {
                let one = v["subject"][0].clone();
                v["subject"] = serde_json::Value::Array(vec![one; edit]);
            });
            assert!(
                err.contains("exactly one subject"),
                "{name} subjects: got: {err}"
            );
        }
    }

    #[test]
    fn the_digest_set_must_hold_exactly_one_entry() {
        let bytes = bundle();

        let err = reject(&bytes, |v| {
            v["subject"][0]["digest"] = serde_json::json!({});
        });
        assert!(err.contains("digest entries"), "zero digests: got: {err}");

        // Two entries is the case the rule exists for: a `DigestSet` matches when ANY field
        // matches, so a second entry offers a second way in.
        let err = reject(&bytes, |v| {
            v["subject"][0]["digest"]["sha512"] = serde_json::json!("00");
        });
        assert!(err.contains("digest entries"), "two digests: got: {err}");
    }

    #[test]
    fn a_digest_under_another_algorithm_is_not_a_sha256() {
        let bytes = bundle();
        let err = reject(&bytes, |v| {
            let d = v["subject"][0]["digest"]["sha256"].clone();
            v["subject"][0]["digest"] = serde_json::json!({ "sha512": d });
        });
        assert!(err.contains("no sha256 entry"), "got: {err}");
    }

    #[test]
    fn a_subject_digest_for_other_bytes_is_refused() {
        let k = key();
        let bytes = bundle();
        let envelope = sign_statement(&statement_for_bundle(&bytes).unwrap(), &k).unwrap();

        let mut other = bytes.clone();
        other.extend_from_slice(b"trailing");
        let err = verify_attestation_for_bundle(&envelope, &k.verifying_key(), &other)
            .expect_err("a different artifact must not match");
        assert!(err.to_string().contains("does not match"), "got: {err}");
    }

    #[test]
    fn a_subject_name_that_is_not_the_bundle_id_is_refused() {
        let bytes = bundle();
        let err = reject(&bytes, |v| {
            v["subject"][0]["name"] = serde_json::json!("some-other-bundle");
        });
        assert!(err.contains("subject name"), "got: {err}");
    }

    // ── predicate fields, one rejection reason each ─────────────────────────────────────────

    /// Every field is mutated on its own, and the rejection must name that field.
    ///
    /// A verifier that rejected for an earlier reason would satisfy a bare `is_err()`, which is
    /// why each case asserts the field name rather than the failure.
    #[test]
    fn each_predicate_field_is_cross_checked_and_named() {
        let bytes = bundle();
        let cases: [(&str, &str, serde_json::Value); 10] = [
            (
                "schema_version",
                "/predicate/schema_version",
                serde_json::json!(2),
            ),
            (
                "semantic_equivalence.algorithm",
                "/predicate/semantic_equivalence/algorithm",
                serde_json::json!("other"),
            ),
            (
                "semantic_equivalence.value",
                "/predicate/semantic_equivalence/value",
                serde_json::json!("00"),
            ),
            (
                "run.run_id",
                "/predicate/run/run_id",
                serde_json::json!("other"),
            ),
            (
                "run.event_count",
                "/predicate/run/event_count",
                serde_json::json!(99),
            ),
            (
                "run.producer.name",
                "/predicate/run/producer/name",
                serde_json::json!("other"),
            ),
            (
                "run.producer.version",
                "/predicate/run/producer/version",
                serde_json::json!("other"),
            ),
            (
                "run.producer.git",
                "/predicate/run/producer/git",
                serde_json::json!("other"),
            ),
            (
                "run.time_window.start",
                "/predicate/run/time_window/start",
                serde_json::json!("2000-01-01T00:00:00Z"),
            ),
            (
                "run.time_window.end",
                "/predicate/run/time_window/end",
                serde_json::json!("2099-01-01T00:00:00Z"),
            ),
        ];

        for (field, pointer, replacement) in cases {
            let err = reject(&bytes, |v| {
                *v.pointer_mut(pointer).expect("pointer resolves") = replacement;
            });
            if field == "schema_version" {
                // Checked before the cross-check, and its own message is the more specific one.
                assert!(err.contains("schema_version"), "{field}: got: {err}");
            } else {
                assert!(
                    err.contains(&format!("`{field}`")),
                    "{field}: expected the field to be named, got: {err}"
                );
            }
        }
    }

    /// The mismatch namer has to stay exhaustive, and this is what makes that a build error.
    ///
    /// Adding a predicate field without extending `first_predicate_mismatch` would leave the
    /// rejection message generic while everything still passed; destructuring here means the
    /// compiler asks for it instead.
    #[test]
    fn the_predicate_has_no_field_the_mismatch_namer_does_not_know() {
        let bytes = bundle();
        let result = crate::bundle::verify_bundle(bytes.as_slice()).unwrap();
        let p = predicate_from_verified(&result).unwrap();

        let EvidenceBundlePredicate {
            schema_version: _,
            semantic_equivalence:
                SemanticEquivalence {
                    algorithm: _,
                    value: _,
                },
            run:
                PredicateRun {
                    run_id: _,
                    event_count: _,
                    producer:
                        PredicateProducer {
                            name: _,
                            version: _,
                            git: _,
                        },
                    time_window: TimeWindow { start: _, end: _ },
                },
        } = p;
    }

    // ── time window ────────────────────────────────────────────────────────────────────────

    #[test]
    fn the_window_keeps_sub_second_precision_and_spans_the_run() {
        let first: DateTime<Utc> = "2026-07-28T10:00:00.123456Z".parse().unwrap();
        let last: DateTime<Utc> = "2026-07-28T10:00:02.987654Z".parse().unwrap();
        // Deliberately out of order on the wire: the window is a min and a max, not the first and
        // last lines of the file.
        let bytes = bundle_with_times(&[last, first]);

        let result = crate::bundle::verify_bundle(bytes.as_slice()).unwrap();
        let p = predicate_from_verified(&result).unwrap();

        assert_eq!(p.run.time_window.start, "2026-07-28T10:00:00.123456Z");
        assert_eq!(p.run.time_window.end, "2026-07-28T10:00:02.987654Z");

        // And the round trip still verifies, so the exact comparison holds on fractional values
        // rather than only on the truncated ones it used to see.
        let k = key();
        let envelope = sign_statement(&statement_for_bundle(&bytes).unwrap(), &k).unwrap();
        assert!(verify_attestation_for_bundle(&envelope, &k.verifying_key(), &bytes).is_ok());
    }

    #[test]
    fn a_verified_bundle_always_has_a_window() {
        let bytes = bundle();
        let result = crate::bundle::verify_bundle(bytes.as_slice()).unwrap();
        // Not an `Option`: the type is the invariant. This case exists to fail compilation if the
        // field ever goes back to one.
        let (start, end) = result.time_window;
        assert!(start <= end);
    }

    // ── limits ─────────────────────────────────────────────────────────────────────────────

    #[test]
    fn a_custom_source_ceiling_is_honoured_by_the_producer() {
        let bytes = bundle();
        let tiny = VerifyLimits {
            max_bundle_bytes: 8,
            ..VerifyLimits::default()
        };
        let err = statement_for_bundle_with_limits(&bytes, tiny)
            .expect_err("a bundle over the source ceiling must not be attested");
        // On the chain, not the top-level context: the outer message says the bundle did not
        // verify, which every rejection says. The ceiling is the reason, and the reason is what
        // this case is about.
        let chain = format!("{err:#}");
        assert!(
            chain.contains("LimitBundleBytes"),
            "expected the source ceiling to be the reason, got: {chain}"
        );
    }
}
