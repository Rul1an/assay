//! MCP04a-3.2b — raw ECDSA-over-bytes + in-toto subject-digest sub-primitives.
//!
//! This slice verifies raw ECDSA-over-bytes and in-toto subject digest binding as sub-primitives. It
//! does **not** verify DSSE envelopes, Sigstore bundles, Rekor inclusion, or PEP740 / npm provenance.
//!
//! DSSE deliberately signs a Pre-Authentication Encoding over `payloadType` + `payload` — the
//! anti-confusion layer that a raw-bytes check does not model. Composing these sub-primitives into DSSE /
//! Sigstore / Rekor semantics is a-3.3, not here. The fixture composition below is named and documented
//! to make that boundary impossible to misread.
//!
//! Status precedence (locked): chain failure wins, then identity, then signature, then subject digest —
//! a valid signature can never make an untrusted chain or a wrong identity clean:
//!
//! 1. chain failure       -> `Failed` / `TrustRootUnavailable`
//! 2. identity failure    -> `IdentityMismatch` / `UnsupportedFormat` / `Failed`
//! 3. signature: non-P256 key / non-DER encoding -> `UnsupportedFormat`; crypto-invalid -> `Failed`
//! 4. subject mismatch    -> `SubjectDigestMismatch`
//! 5. only then           -> `Verified`

use std::collections::BTreeMap;

use ecdsa::signature::Verifier;
use p256::ecdsa::{Signature, VerifyingKey};
use p256::pkcs8::DecodePublicKey;
use x509_cert::der::{Decode, Encode};
use x509_cert::Certificate;

use crate::sigstore_identity::{verify_identity_offline, ExpectedIdentity};
use crate::supply_chain::CheckStatus;

/// The outcome of a signature / subject-digest sub-primitive: a `CheckStatus` plus a value-free reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureOutcome {
    pub status: CheckStatus,
    pub reason: &'static str,
}

impl SignatureOutcome {
    fn new(status: CheckStatus, reason: &'static str) -> Self {
        Self { status, reason }
    }
}

/// Verify a raw **DER-encoded** ECDSA/P-256 signature over `message` using the public key embedded in
/// `leaf_der`.
///
/// This is a low-level cryptographic primitive. It does NOT apply DSSE PAE and does NOT parse any
/// envelope — `message` is verified verbatim. The signature encoding accepted is ASN.1 DER (not
/// fixed-width IEEE P1363). The two failure axes are kept distinct: a non-P256 leaf key or a
/// non-DER/malformed signature *encoding* is `UnsupportedFormat`; a well-formed signature that is
/// cryptographically invalid (tampered, or made by another key) is `Failed`.
pub fn verify_leaf_ecdsa_signature_over_bytes(
    leaf_der: &[u8],
    message: &[u8],
    signature_der: &[u8],
) -> SignatureOutcome {
    let cert = match Certificate::from_der(leaf_der) {
        Ok(c) => c,
        Err(_) => return SignatureOutcome::new(CheckStatus::Failed, "malformed leaf certificate"),
    };
    let spki_der = match cert.tbs_certificate.subject_public_key_info.to_der() {
        Ok(d) => d,
        Err(_) => {
            return SignatureOutcome::new(CheckStatus::Failed, "malformed leaf public key info")
        }
    };
    let vk = match VerifyingKey::from_public_key_der(&spki_der) {
        Ok(k) => k,
        Err(_) => {
            return SignatureOutcome::new(
                CheckStatus::UnsupportedFormat,
                "leaf public key is not ECDSA P-256",
            )
        }
    };
    let sig = match Signature::from_der(signature_der) {
        Ok(s) => s,
        Err(_) => {
            return SignatureOutcome::new(
                CheckStatus::UnsupportedFormat,
                "signature is not a DER-encoded ECDSA signature",
            )
        }
    };
    match vk.verify(message, &sig) {
        Ok(()) => SignatureOutcome::new(
            CheckStatus::Verified,
            "signature verifies under the leaf public key",
        ),
        Err(_) => SignatureOutcome::new(
            CheckStatus::Failed,
            "signature does not verify under the leaf public key",
        ),
    }
}

#[derive(serde::Deserialize)]
struct InTotoStatement {
    #[serde(default)]
    subject: Vec<InTotoSubject>,
}

#[derive(serde::Deserialize)]
struct InTotoSubject {
    #[serde(default)]
    digest: BTreeMap<String, String>,
}

/// Parse an in-toto Statement and bind its single subject's `sha256` digest to `expected_sha256` (hex,
/// case-insensitive).
///
/// This reads the Statement's `subject[].digest` only — it is NOT a DSSE / Sigstore check. It binds a
/// **single** subject: a statement with anything other than exactly one subject is `UnsupportedFormat`,
/// because "some subject carries the expected digest" is unsafe — a decoy subject could carry the
/// expected digest while the real artifact subject does not. Subject-NAME binding (matching a specific
/// subject) is a later carrier/adapter concern, not this primitive. A subject whose digest is a
/// non-sha256 algorithm is `UnsupportedFormat`; a subject with no digest at all (or a statement that does
/// not parse) is `Failed`.
pub fn bind_in_toto_subject_digest(
    statement_json: &[u8],
    expected_sha256: &str,
) -> SignatureOutcome {
    let statement: InTotoStatement = match serde_json::from_slice(statement_json) {
        Ok(s) => s,
        Err(_) => return SignatureOutcome::new(CheckStatus::Failed, "malformed in-toto statement"),
    };

    // Exactly one subject. More than one is ambiguous (which subject is "the" artifact?) and accepting a
    // match from any of them would let a decoy subject launder the digest; zero subjects has nothing to
    // bind.
    let subject = match statement.subject.as_slice() {
        [only] => only,
        _ => {
            return SignatureOutcome::new(
                CheckStatus::UnsupportedFormat,
                "statement does not carry exactly one subject",
            )
        }
    };

    match subject.digest.get("sha256") {
        Some(sha256) if sha256.eq_ignore_ascii_case(expected_sha256) => SignatureOutcome::new(
            CheckStatus::Verified,
            "subject sha256 matches the expected artifact digest",
        ),
        Some(_) => SignatureOutcome::new(
            CheckStatus::SubjectDigestMismatch,
            "subject sha256 does not match the expected artifact digest",
        ),
        None if !subject.digest.is_empty() => SignatureOutcome::new(
            CheckStatus::UnsupportedFormat,
            "subject digest algorithm is not sha256",
        ),
        None => SignatureOutcome::new(CheckStatus::Failed, "statement has no subject digest"),
    }
}

/// Fixture-level raw statement signature check.
/// Not DSSE envelope verification. Not Sigstore bundle verification. Does not apply DSSE PAE.
///
/// Composes the a-3.2a identity check with the two sub-primitives above, in strict precedence order
/// (chain -> identity -> signature -> subject digest). `signature_der` is verified over the raw
/// `statement_json` bytes; `statement_json` is then parsed for its subject sha256 and bound to
/// `expected_sha256`. A `Verified` result means: the leaf chains to a pinned root, binds to `expected`,
/// signed these exact statement bytes, and the statement commits to the expected artifact digest — at
/// the fixture level, with no DSSE / Sigstore / Rekor semantics applied.
#[allow(clippy::too_many_arguments)]
pub fn verify_identity_bound_raw_statement_fixture(
    leaf_der: &[u8],
    intermediates: &[&[u8]],
    pinned_roots: &[&[u8]],
    now_unix_secs: u64,
    expected: &ExpectedIdentity<'_>,
    statement_json: &[u8],
    signature_der: &[u8],
    expected_sha256: &str,
) -> SignatureOutcome {
    // 1+2: chain and identity are prerequisites. A valid signature must never override an untrusted
    // chain or a wrong identity, so any non-Verified identity status short-circuits here.
    let identity = verify_identity_offline(
        leaf_der,
        intermediates,
        pinned_roots,
        now_unix_secs,
        expected,
    );
    if identity.status != CheckStatus::Verified {
        return SignatureOutcome::new(identity.status, identity.reason);
    }

    // 3: the leaf must have signed these exact statement bytes.
    let signature = verify_leaf_ecdsa_signature_over_bytes(leaf_der, statement_json, signature_der);
    if signature.status != CheckStatus::Verified {
        return signature;
    }

    // 4: the statement must commit to the expected artifact digest.
    let digest = bind_in_toto_subject_digest(statement_json, expected_sha256);
    if digest.status != CheckStatus::Verified {
        return digest;
    }

    // 5: everything checks out (at the fixture level).
    SignatureOutcome::new(
        CheckStatus::Verified,
        "identity, raw signature, and subject digest all verify (fixture level)",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ecdsa::signature::Signer;
    use p256::ecdsa::SigningKey;
    use p256::pkcs8::DecodePrivateKey;
    use rcgen::{
        BasicConstraints, CertificateParams, CustomExtension, DistinguishedName, DnType,
        ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair, KeyUsagePurpose, SanType,
        PKCS_ECDSA_P256_SHA256, PKCS_ED25519,
    };
    use time::{Duration, OffsetDateTime};

    const NOW: u64 = 1_750_000_000;
    const SAN_URI: &str =
        "https://github.com/example/repo/.github/workflows/release.yml@refs/tags/v1";
    const ISSUER: &str = "https://token.actions.githubusercontent.com";
    const FULCIO_V2_OID: &[u64] = &[1, 3, 6, 1, 4, 1, 57264, 1, 8];
    // sha256 of the empty input; a stable, real 64-hex digest for the fixtures.
    const ARTIFACT_SHA256: &str =
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    fn at(secs: i64) -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(secs).unwrap()
    }

    fn der_utf8(s: &str) -> Vec<u8> {
        let b = s.as_bytes();
        assert!(b.len() < 128, "test issuer too long for short-form length");
        let mut v = vec![0x0c, b.len() as u8];
        v.extend_from_slice(b);
        v
    }

    fn expected() -> ExpectedIdentity<'static> {
        ExpectedIdentity {
            san: SAN_URI,
            issuer: ISSUER,
        }
    }

    /// Build a root CA + a Fulcio-shaped P-256 leaf (URI `san`, v2 issuer, code-signing EKU, empty
    /// subject) and return `(root_der, leaf_der, leaf_keypair)`. The keypair lets tests produce real
    /// signatures under the leaf's public key.
    fn build_p256(san: &str) -> (Vec<u8>, Vec<u8>, KeyPair) {
        let nb = at(NOW as i64) - Duration::days(1);
        let na = at(NOW as i64) + Duration::days(1);

        let root_key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).unwrap();
        let mut root_params = CertificateParams::new(Vec::<String>::new()).unwrap();
        let mut root_dn = DistinguishedName::new();
        root_dn.push(DnType::CommonName, "Assay Test Root CA");
        root_params.distinguished_name = root_dn;
        root_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        root_params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
        root_params.not_before = nb;
        root_params.not_after = na;
        let root_cert = root_params.self_signed(&root_key).unwrap();

        let leaf_key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).unwrap();
        let mut leaf_params = CertificateParams::new(Vec::<String>::new()).unwrap();
        leaf_params.distinguished_name = DistinguishedName::new();
        leaf_params.subject_alt_names = vec![SanType::URI(san.try_into().unwrap())];
        leaf_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::CodeSigning];
        leaf_params
            .custom_extensions
            .push(CustomExtension::from_oid_content(
                FULCIO_V2_OID,
                der_utf8(ISSUER),
            ));
        leaf_params.not_before = nb;
        leaf_params.not_after = na;
        let issuer = Issuer::from_params(&root_params, &root_key);
        let leaf_cert = leaf_params.signed_by(&leaf_key, &issuer).unwrap();

        (root_cert.der().to_vec(), leaf_cert.der().to_vec(), leaf_key)
    }

    /// A self-signed Ed25519 leaf — used to exercise the non-P256 key path. We only need its DER.
    fn ed25519_leaf_der() -> Vec<u8> {
        let key = KeyPair::generate_for(&PKCS_ED25519).unwrap();
        let mut params = CertificateParams::new(Vec::<String>::new()).unwrap();
        params.distinguished_name = DistinguishedName::new();
        params.subject_alt_names = vec![SanType::URI(SAN_URI.try_into().unwrap())];
        params.self_signed(&key).unwrap().der().to_vec()
    }

    /// Sign `msg` with the leaf keypair, returning a DER-encoded ECDSA signature.
    fn sign(leaf_key: &KeyPair, msg: &[u8]) -> Vec<u8> {
        let pkcs8 = leaf_key.serialize_der();
        let sk = SigningKey::from_pkcs8_der(&pkcs8).unwrap();
        let sig: Signature = sk.sign(msg);
        sig.to_der().as_bytes().to_vec()
    }

    fn statement(sha256: &str) -> Vec<u8> {
        format!(
            r#"{{"_type":"https://in-toto.io/Statement/v1","subject":[{{"name":"artifact","digest":{{"sha256":"{sha256}"}}}}],"predicateType":"x","predicate":{{}}}}"#
        )
        .into_bytes()
    }

    // --- raw ECDSA primitive ---

    #[test]
    fn signature_verifies_under_leaf_spki() {
        let (_root, leaf, key) = build_p256(SAN_URI);
        let msg = statement(ARTIFACT_SHA256);
        let sig = sign(&key, &msg);
        let out = verify_leaf_ecdsa_signature_over_bytes(&leaf, &msg, &sig);
        assert_eq!(out.status, CheckStatus::Verified, "{}", out.reason);
    }

    #[test]
    fn tampered_signature_fails() {
        let (_root, leaf, key) = build_p256(SAN_URI);
        let msg = statement(ARTIFACT_SHA256);
        let mut sig = sign(&key, &msg);
        *sig.last_mut().unwrap() ^= 0x01; // corrupt the final signature byte
        let out = verify_leaf_ecdsa_signature_over_bytes(&leaf, &msg, &sig);
        assert_eq!(out.status, CheckStatus::Failed, "{}", out.reason);
    }

    #[test]
    fn signature_by_other_key_fails() {
        let (_root, leaf, _key) = build_p256(SAN_URI);
        let (_r2, _l2, other_key) = build_p256(SAN_URI);
        let msg = statement(ARTIFACT_SHA256);
        let sig = sign(&other_key, &msg); // signed by a different leaf's key
        let out = verify_leaf_ecdsa_signature_over_bytes(&leaf, &msg, &sig);
        assert_eq!(out.status, CheckStatus::Failed, "{}", out.reason);
    }

    #[test]
    fn non_p256_leaf_is_unsupported_format() {
        let leaf = ed25519_leaf_der();
        let out = verify_leaf_ecdsa_signature_over_bytes(&leaf, b"anything", &[0x30, 0x06]);
        assert_eq!(out.status, CheckStatus::UnsupportedFormat, "{}", out.reason);
    }

    #[test]
    fn malformed_signature_encoding_is_unsupported_format() {
        // A P-256 leaf, but the signature bytes are not a DER-encoded ECDSA signature. This is an
        // encoding problem (UnsupportedFormat), distinct from a well-formed-but-invalid signature.
        let (_root, leaf, _key) = build_p256(SAN_URI);
        let out = verify_leaf_ecdsa_signature_over_bytes(&leaf, b"msg", b"not-a-der-signature");
        assert_eq!(out.status, CheckStatus::UnsupportedFormat, "{}", out.reason);
    }

    // --- in-toto subject digest binder ---

    #[test]
    fn subject_sha256_matches_verifies() {
        let out = bind_in_toto_subject_digest(&statement(ARTIFACT_SHA256), ARTIFACT_SHA256);
        assert_eq!(out.status, CheckStatus::Verified, "{}", out.reason);
    }

    #[test]
    fn subject_digest_mismatch() {
        let other = "0000000000000000000000000000000000000000000000000000000000000000";
        let out = bind_in_toto_subject_digest(&statement(other), ARTIFACT_SHA256);
        assert_eq!(
            out.status,
            CheckStatus::SubjectDigestMismatch,
            "{}",
            out.reason
        );
    }

    #[test]
    fn non_sha256_digest_is_unsupported_format() {
        let stmt = br#"{"subject":[{"name":"a","digest":{"sha512":"deadbeef"}}]}"#;
        let out = bind_in_toto_subject_digest(stmt, ARTIFACT_SHA256);
        assert_eq!(out.status, CheckStatus::UnsupportedFormat, "{}", out.reason);
    }

    #[test]
    fn missing_digest_fails() {
        let stmt = br#"{"subject":[{"name":"a","digest":{}}]}"#;
        let out = bind_in_toto_subject_digest(stmt, ARTIFACT_SHA256);
        assert_eq!(out.status, CheckStatus::Failed, "{}", out.reason);
    }

    #[test]
    fn multiple_subjects_is_unsupported_format() {
        // Decoy-subject attack: a decoy subject carries the expected digest while the real artifact
        // subject does not. Accepting "some subject matches" would launder the digest, so a multi-subject
        // statement is refused outright rather than scanned for any match.
        let stmt = format!(
            r#"{{"subject":[{{"name":"decoy","digest":{{"sha256":"{ARTIFACT_SHA256}"}}}},{{"name":"real-artifact","digest":{{"sha256":"0000000000000000000000000000000000000000000000000000000000000000"}}}}]}}"#
        );
        let out = bind_in_toto_subject_digest(stmt.as_bytes(), ARTIFACT_SHA256);
        assert_eq!(out.status, CheckStatus::UnsupportedFormat, "{}", out.reason);
    }

    #[test]
    fn malformed_statement_fails() {
        let out = bind_in_toto_subject_digest(b"not json", ARTIFACT_SHA256);
        assert_eq!(out.status, CheckStatus::Failed, "{}", out.reason);
    }

    // --- composition precedence ---

    #[test]
    fn full_chain_identity_signature_digest_verifies() {
        let (root, leaf, key) = build_p256(SAN_URI);
        let msg = statement(ARTIFACT_SHA256);
        let sig = sign(&key, &msg);
        let out = verify_identity_bound_raw_statement_fixture(
            &leaf,
            &[],
            &[&root],
            NOW,
            &expected(),
            &msg,
            &sig,
            ARTIFACT_SHA256,
        );
        assert_eq!(out.status, CheckStatus::Verified, "{}", out.reason);
    }

    #[test]
    fn valid_identity_signature_subject_mismatch_is_subject_digest_mismatch() {
        let (root, leaf, key) = build_p256(SAN_URI);
        let other = "0000000000000000000000000000000000000000000000000000000000000000";
        let msg = statement(other); // signed statement commits to a different digest
        let sig = sign(&key, &msg);
        let out = verify_identity_bound_raw_statement_fixture(
            &leaf,
            &[],
            &[&root],
            NOW,
            &expected(),
            &msg,
            &sig,
            ARTIFACT_SHA256,
        );
        assert_eq!(
            out.status,
            CheckStatus::SubjectDigestMismatch,
            "{}",
            out.reason
        );
    }

    #[test]
    fn wrong_identity_valid_signature_is_identity_mismatch() {
        let (root, leaf, key) = build_p256("https://github.com/evil/repo/x.yml@refs/heads/main");
        let msg = statement(ARTIFACT_SHA256);
        let sig = sign(&key, &msg); // a perfectly valid signature over a matching-digest statement
        let out = verify_identity_bound_raw_statement_fixture(
            &leaf,
            &[],
            &[&root],
            NOW,
            &expected(),
            &msg,
            &sig,
            ARTIFACT_SHA256,
        );
        assert_eq!(out.status, CheckStatus::IdentityMismatch, "{}", out.reason);
    }

    #[test]
    fn bad_chain_valid_signature_is_failed() {
        let (_root, leaf, key) = build_p256(SAN_URI);
        let (other_root, _l, _k) = build_p256(SAN_URI); // unrelated CA
        let msg = statement(ARTIFACT_SHA256);
        let sig = sign(&key, &msg);
        let out = verify_identity_bound_raw_statement_fixture(
            &leaf,
            &[],
            &[&other_root],
            NOW,
            &expected(),
            &msg,
            &sig,
            ARTIFACT_SHA256,
        );
        assert_eq!(out.status, CheckStatus::Failed, "{}", out.reason);
    }

    #[test]
    fn valid_identity_tampered_signature_is_failed() {
        let (root, leaf, key) = build_p256(SAN_URI);
        let msg = statement(ARTIFACT_SHA256);
        let mut sig = sign(&key, &msg);
        *sig.last_mut().unwrap() ^= 0x01;
        let out = verify_identity_bound_raw_statement_fixture(
            &leaf,
            &[],
            &[&root],
            NOW,
            &expected(),
            &msg,
            &sig,
            ARTIFACT_SHA256,
        );
        assert_eq!(out.status, CheckStatus::Failed, "{}", out.reason);
    }
}
