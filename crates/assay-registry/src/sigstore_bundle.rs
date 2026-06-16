//! MCP04a-3.3b — Sigstore DSSE bundle composition boundary (offline).
//!
//! Verifies a **constrained** Sigstore bundle shape by routing its parts to the existing primitives:
//! a v0.3 JSON bundle whose verification material is a single leaf `certificate` and whose content is a
//! `dsseEnvelope`. It does NOT verify Rekor inclusion, live transparency-log state, timestamp freshness,
//! PEP740/npm adapter semantics, VSA output, registry-ecosystem security, or code safety.
//!
//! Trust model (load-bearing): the **bundle supplies only the evidence leaf**. Intermediates and roots are
//! the verifier's PINNED trust material (local / TUF-derived), never taken from the bundle — a bundle may
//! not supply its own trust chain. This is also why `x509CertificateChain` is rejected on the v0.3 path.
//!
//! Composition:
//! - `verificationMaterial.certificate` (leaf) + pinned intermediates + pinned roots -> a-3.1 chain +
//!   a-3.2a identity
//! - `dsseEnvelope` -> a-3.3a DSSE/PAE verification + a-3.2b in-toto subject-digest binding
//!
//! Precedence (locked): unsupported bundle shape -> chain/trust -> identity -> DSSE signature -> subject
//! digest -> verified. A valid DSSE envelope can never make a wrong identity or an untrusted chain clean.
//!
//! **A verified bundle here means chain + identity + DSSE/PAE + subject digest matched under pinned roots
//! and the supplied policy. It does NOT mean the artifact is safe, nor that transparency-log inclusion was
//! verified.**

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

use crate::dsse::verify_dsse_envelope_offline;
use crate::sigstore_identity::{verify_identity_offline, ExpectedIdentity};
use crate::supply_chain::CheckStatus;

/// The only Sigstore bundle media type this slice accepts.
const BUNDLE_MEDIA_TYPE_V0_3: &str = "application/vnd.dev.sigstore.bundle.v0.3+json";

/// The outcome of offline bundle composition: a `CheckStatus` plus a value-free reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleOutcome {
    pub status: CheckStatus,
    pub reason: &'static str,
}

impl BundleOutcome {
    fn new(status: CheckStatus, reason: &'static str) -> Self {
        Self { status, reason }
    }
}

#[derive(serde::Deserialize)]
struct BundleIn {
    #[serde(rename = "mediaType")]
    media_type: Option<String>,
    #[serde(rename = "verificationMaterial")]
    verification_material: Option<VerificationMaterialIn>,
    #[serde(rename = "dsseEnvelope")]
    dsse_envelope: Option<serde_json::Value>,
    #[serde(rename = "messageSignature")]
    message_signature: Option<serde_json::Value>,
}

#[derive(serde::Deserialize)]
struct VerificationMaterialIn {
    certificate: Option<X509CertificateIn>,
    #[serde(rename = "x509CertificateChain")]
    x509_certificate_chain: Option<serde_json::Value>,
    #[serde(rename = "publicKey")]
    public_key: Option<serde_json::Value>,
}

#[derive(serde::Deserialize)]
struct X509CertificateIn {
    #[serde(rename = "rawBytes")]
    raw_bytes: Option<String>,
}

/// Verify a constrained Sigstore DSSE bundle offline, composing the existing primitives.
///
/// The bundle provides the leaf certificate and the DSSE envelope; `pinned_intermediates` and
/// `pinned_roots` are the verifier's own trust material (the bundle is never trusted to supply its chain).
/// See the module docs for the exact constrained shape and the explicit non-claims (no Rekor, no
/// transparency, etc.).
#[allow(clippy::too_many_arguments)]
pub fn verify_sigstore_dsse_bundle_offline(
    bundle_json: &[u8],
    pinned_intermediates: &[&[u8]],
    pinned_roots: &[&[u8]],
    now_unix_secs: u64,
    expected: &ExpectedIdentity<'_>,
    expected_sha256: &str,
) -> BundleOutcome {
    let bundle: BundleIn = match serde_json::from_slice(bundle_json) {
        Ok(b) => b,
        Err(_) => {
            return BundleOutcome::new(CheckStatus::UnsupportedFormat, "malformed Sigstore bundle")
        }
    };

    // --- shape gate (unsupported_format wins first) ---
    match bundle.media_type.as_deref() {
        Some(BUNDLE_MEDIA_TYPE_V0_3) => {}
        _ => {
            return BundleOutcome::new(
                CheckStatus::UnsupportedFormat,
                "unsupported or missing bundle mediaType (only v0.3 is accepted)",
            )
        }
    }

    // Content oneof: exactly a dsseEnvelope. messageSignature (alone or alongside) is out of scope.
    if bundle.message_signature.is_some() {
        return BundleOutcome::new(
            CheckStatus::UnsupportedFormat,
            "messageSignature bundles are not supported (dsseEnvelope only)",
        );
    }
    let dsse_value = match bundle.dsse_envelope {
        Some(v) => v,
        None => return BundleOutcome::new(CheckStatus::Failed, "bundle has no content"),
    };

    // Verification material: a single leaf certificate. The bundle must not supply its own chain.
    let material = match bundle.verification_material {
        Some(m) => m,
        None => {
            return BundleOutcome::new(CheckStatus::Failed, "bundle has no verificationMaterial")
        }
    };
    if material.x509_certificate_chain.is_some() {
        return BundleOutcome::new(
            CheckStatus::UnsupportedFormat,
            "x509CertificateChain is not supported on the v0.3 path (trust material is pinned)",
        );
    }
    if material.public_key.is_some() {
        return BundleOutcome::new(
            CheckStatus::UnsupportedFormat,
            "publicKey verification material is not supported (certificate only)",
        );
    }
    let raw_bytes = match material.certificate.and_then(|c| c.raw_bytes) {
        Some(b) => b,
        None => {
            return BundleOutcome::new(
                CheckStatus::UnsupportedFormat,
                "verificationMaterial has no supported certificate",
            )
        }
    };
    let leaf_der = match BASE64.decode(raw_bytes.as_bytes()) {
        Ok(b) => b,
        Err(_) => return BundleOutcome::new(CheckStatus::Failed, "malformed leaf certificate"),
    };

    // --- chain + identity (a-3.1 + a-3.2a) using PINNED intermediates/roots only ---
    let identity = verify_identity_offline(
        &leaf_der,
        pinned_intermediates,
        pinned_roots,
        now_unix_secs,
        expected,
    );
    if identity.status != CheckStatus::Verified {
        return BundleOutcome::new(identity.status, identity.reason);
    }

    // --- DSSE/PAE + subject digest (a-3.3a, which composes a-3.2b) over the bundle content ---
    let dsse_bytes = match serde_json::to_vec(&dsse_value) {
        Ok(b) => b,
        Err(_) => return BundleOutcome::new(CheckStatus::Failed, "malformed dsseEnvelope"),
    };
    let dsse = verify_dsse_envelope_offline(&leaf_der, &dsse_bytes, expected_sha256);
    if dsse.status != CheckStatus::Verified {
        return BundleOutcome::new(dsse.status, dsse.reason);
    }

    // Note: any tlogEntries / transparency material in the bundle is intentionally NOT inspected here;
    // Rekor inclusion is a-3.3c. A Verified result makes no transparency-log claim.
    BundleOutcome::new(
        CheckStatus::Verified,
        "bundle leaf chains to a pinned root, binds to the expected identity, and its DSSE envelope verifies",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ecdsa::signature::Signer;
    use p256::ecdsa::{Signature, SigningKey};
    use p256::pkcs8::DecodePrivateKey;
    use rcgen::{
        BasicConstraints, CertificateParams, CustomExtension, DistinguishedName, DnType,
        ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair, KeyUsagePurpose, SanType,
        PKCS_ECDSA_P256_SHA256,
    };
    use time::{Duration, OffsetDateTime};

    const NOW: u64 = 1_750_000_000;
    const SAN_URI: &str =
        "https://github.com/example/repo/.github/workflows/release.yml@refs/tags/v1";
    const ISSUER: &str = "https://token.actions.githubusercontent.com";
    const IN_TOTO_TYPE: &str = "application/vnd.in-toto+json";
    const FULCIO_V2_OID: &[u64] = &[1, 3, 6, 1, 4, 1, 57264, 1, 8];
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

    struct Pki {
        root: Vec<u8>,
        intermediate: Vec<u8>,
        leaf: Vec<u8>,
        leaf_key: KeyPair,
    }

    /// Build root -> intermediate -> leaf (Fulcio-shaped, P-256). The leaf carries one URI SAN + the v2
    /// issuer extension + code-signing EKU; the intermediate carries the code-signing EKU (Fulcio
    /// profile). Returns the leaf keypair so tests can produce real DSSE signatures.
    fn build_pki(leaf_san: &str) -> Pki {
        let nb = at(NOW as i64) - Duration::days(1);
        let na = at(NOW as i64) + Duration::days(1);
        let ca_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
        let dn = |cn: &str| {
            let mut d = DistinguishedName::new();
            d.push(DnType::CommonName, cn);
            d
        };

        let root_key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).unwrap();
        let mut root_params = CertificateParams::new(Vec::<String>::new()).unwrap();
        root_params.distinguished_name = dn("Assay Test Root CA");
        root_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        root_params.key_usages = ca_usages.clone();
        root_params.not_before = nb;
        root_params.not_after = na;
        let root_cert = root_params.self_signed(&root_key).unwrap();

        let int_key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).unwrap();
        let mut int_params = CertificateParams::new(Vec::<String>::new()).unwrap();
        int_params.distinguished_name = dn("Assay Test Intermediate CA");
        int_params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
        int_params.key_usages = ca_usages;
        int_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::CodeSigning];
        int_params.not_before = nb;
        int_params.not_after = na;
        let root_issuer = Issuer::from_params(&root_params, &root_key);
        let int_cert = int_params.signed_by(&int_key, &root_issuer).unwrap();

        let leaf_key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).unwrap();
        let mut leaf_params = CertificateParams::new(Vec::<String>::new()).unwrap();
        leaf_params.distinguished_name = DistinguishedName::new();
        leaf_params.subject_alt_names = vec![SanType::URI(leaf_san.try_into().unwrap())];
        leaf_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::CodeSigning];
        leaf_params
            .custom_extensions
            .push(CustomExtension::from_oid_content(
                FULCIO_V2_OID,
                der_utf8(ISSUER),
            ));
        leaf_params.not_before = nb;
        leaf_params.not_after = na;
        let int_issuer = Issuer::from_params(&int_params, &int_key);
        let leaf_cert = leaf_params.signed_by(&leaf_key, &int_issuer).unwrap();

        Pki {
            root: root_cert.der().to_vec(),
            intermediate: int_cert.der().to_vec(),
            leaf: leaf_cert.der().to_vec(),
            leaf_key,
        }
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

    /// DSSE v1 PAE (mirrors the dsse module; tests build their own signed bytes).
    fn pae(payload_type: &str, payload: &[u8]) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(b"DSSEv1 ");
        p.extend_from_slice(payload_type.len().to_string().as_bytes());
        p.push(b' ');
        p.extend_from_slice(payload_type.as_bytes());
        p.push(b' ');
        p.extend_from_slice(payload.len().to_string().as_bytes());
        p.push(b' ');
        p.extend_from_slice(payload);
        p
    }

    fn dsse_envelope(payload: &[u8], sig: &[u8]) -> String {
        format!(
            r#"{{"payloadType":"{}","payload":"{}","signatures":[{{"keyid":"","sig":"{}"}}]}}"#,
            IN_TOTO_TYPE,
            BASE64.encode(payload),
            BASE64.encode(sig)
        )
    }

    /// Assemble a v0.3 bundle with a single leaf certificate and a dsseEnvelope content.
    fn bundle(media_type: &str, leaf_der: &[u8], dsse_envelope_json: &str) -> Vec<u8> {
        format!(
            r#"{{"mediaType":"{}","verificationMaterial":{{"certificate":{{"rawBytes":"{}"}}}},"dsseEnvelope":{}}}"#,
            media_type,
            BASE64.encode(leaf_der),
            dsse_envelope_json
        )
        .into_bytes()
    }

    /// A fully valid bundle + its pinned trust material.
    fn valid_bundle() -> (Vec<u8>, Pki) {
        let pki = build_pki(SAN_URI);
        let stmt = statement(ARTIFACT_SHA256);
        let sig = sign(&pki.leaf_key, &pae(IN_TOTO_TYPE, &stmt));
        let env = dsse_envelope(&stmt, &sig);
        let b = bundle(BUNDLE_MEDIA_TYPE_V0_3, &pki.leaf, &env);
        (b, pki)
    }

    fn verify(b: &[u8], pki: &Pki) -> BundleOutcome {
        verify_sigstore_dsse_bundle_offline(
            b,
            &[&pki.intermediate],
            &[&pki.root],
            NOW,
            &expected(),
            ARTIFACT_SHA256,
        )
    }

    #[test]
    fn valid_v0_3_bundle_verifies() {
        let (b, pki) = valid_bundle();
        let out = verify(&b, &pki);
        assert_eq!(out.status, CheckStatus::Verified, "{}", out.reason);
    }

    #[test]
    fn wrong_media_type_is_unsupported_format() {
        let pki = build_pki(SAN_URI);
        let stmt = statement(ARTIFACT_SHA256);
        let sig = sign(&pki.leaf_key, &pae(IN_TOTO_TYPE, &stmt));
        let b = bundle(
            "application/vnd.dev.sigstore.bundle+json;version=0.2",
            &pki.leaf,
            &dsse_envelope(&stmt, &sig),
        );
        let out = verify(&b, &pki);
        assert_eq!(out.status, CheckStatus::UnsupportedFormat, "{}", out.reason);
    }

    #[test]
    fn missing_media_type_is_unsupported_format() {
        let pki = build_pki(SAN_URI);
        let stmt = statement(ARTIFACT_SHA256);
        let sig = sign(&pki.leaf_key, &pae(IN_TOTO_TYPE, &stmt));
        let b = format!(
            r#"{{"verificationMaterial":{{"certificate":{{"rawBytes":"{}"}}}},"dsseEnvelope":{}}}"#,
            BASE64.encode(&pki.leaf),
            dsse_envelope(&stmt, &sig)
        );
        let out = verify(b.as_bytes(), &pki);
        assert_eq!(out.status, CheckStatus::UnsupportedFormat, "{}", out.reason);
    }

    #[test]
    fn message_signature_content_is_unsupported_format() {
        let pki = build_pki(SAN_URI);
        let b = format!(
            r#"{{"mediaType":"{BUNDLE_MEDIA_TYPE_V0_3}","verificationMaterial":{{"certificate":{{"rawBytes":"{}"}}}},"messageSignature":{{"signature":"AA=="}}}}"#,
            BASE64.encode(&pki.leaf)
        );
        let out = verify(b.as_bytes(), &pki);
        assert_eq!(out.status, CheckStatus::UnsupportedFormat, "{}", out.reason);
    }

    #[test]
    fn public_key_material_is_unsupported_format() {
        let pki = build_pki(SAN_URI);
        let stmt = statement(ARTIFACT_SHA256);
        let sig = sign(&pki.leaf_key, &pae(IN_TOTO_TYPE, &stmt));
        let b = format!(
            r#"{{"mediaType":"{BUNDLE_MEDIA_TYPE_V0_3}","verificationMaterial":{{"publicKey":{{"hint":"x"}}}},"dsseEnvelope":{}}}"#,
            dsse_envelope(&stmt, &sig)
        );
        let out = verify(b.as_bytes(), &pki);
        assert_eq!(out.status, CheckStatus::UnsupportedFormat, "{}", out.reason);
    }

    #[test]
    fn x509_chain_in_v0_3_is_unsupported_format() {
        let pki = build_pki(SAN_URI);
        let stmt = statement(ARTIFACT_SHA256);
        let sig = sign(&pki.leaf_key, &pae(IN_TOTO_TYPE, &stmt));
        let b = format!(
            r#"{{"mediaType":"{BUNDLE_MEDIA_TYPE_V0_3}","verificationMaterial":{{"x509CertificateChain":{{"certificates":[{{"rawBytes":"{}"}}]}}}},"dsseEnvelope":{}}}"#,
            BASE64.encode(&pki.leaf),
            dsse_envelope(&stmt, &sig)
        );
        let out = verify(b.as_bytes(), &pki);
        assert_eq!(out.status, CheckStatus::UnsupportedFormat, "{}", out.reason);
    }

    #[test]
    fn missing_certificate_is_unsupported_format() {
        let pki = build_pki(SAN_URI);
        let stmt = statement(ARTIFACT_SHA256);
        let sig = sign(&pki.leaf_key, &pae(IN_TOTO_TYPE, &stmt));
        let b = format!(
            r#"{{"mediaType":"{BUNDLE_MEDIA_TYPE_V0_3}","verificationMaterial":{{}},"dsseEnvelope":{}}}"#,
            dsse_envelope(&stmt, &sig)
        );
        let out = verify(b.as_bytes(), &pki);
        assert_eq!(out.status, CheckStatus::UnsupportedFormat, "{}", out.reason);
    }

    #[test]
    fn missing_verification_material_fails() {
        let pki = build_pki(SAN_URI);
        let stmt = statement(ARTIFACT_SHA256);
        let sig = sign(&pki.leaf_key, &pae(IN_TOTO_TYPE, &stmt));
        let b = format!(
            r#"{{"mediaType":"{BUNDLE_MEDIA_TYPE_V0_3}","dsseEnvelope":{}}}"#,
            dsse_envelope(&stmt, &sig)
        );
        let out = verify(b.as_bytes(), &pki);
        assert_eq!(out.status, CheckStatus::Failed, "{}", out.reason);
    }

    /// THE load-bearing bundle test: a valid chain + valid DSSE + matching digest but the WRONG identity
    /// is `IdentityMismatch`, never `Verified`. A valid envelope cannot launder a wrong identity.
    #[test]
    fn valid_chain_wrong_identity_is_identity_mismatch() {
        let pki = build_pki("https://github.com/evil/repo/x.yml@refs/heads/main");
        let stmt = statement(ARTIFACT_SHA256);
        let sig = sign(&pki.leaf_key, &pae(IN_TOTO_TYPE, &stmt));
        let b = bundle(
            BUNDLE_MEDIA_TYPE_V0_3,
            &pki.leaf,
            &dsse_envelope(&stmt, &sig),
        );
        let out = verify(&b, &pki);
        assert_eq!(out.status, CheckStatus::IdentityMismatch, "{}", out.reason);
    }

    #[test]
    fn chain_invalid_without_intermediate_fails() {
        let (b, pki) = valid_bundle();
        // Withhold the pinned intermediate: the leaf cannot chain to the pinned root.
        let out = verify_sigstore_dsse_bundle_offline(
            &b,
            &[],
            &[&pki.root],
            NOW,
            &expected(),
            ARTIFACT_SHA256,
        );
        assert_eq!(out.status, CheckStatus::Failed, "{}", out.reason);
    }

    #[test]
    fn trusted_root_withheld_is_trust_root_unavailable() {
        let (b, pki) = valid_bundle();
        let out = verify_sigstore_dsse_bundle_offline(
            &b,
            &[&pki.intermediate],
            &[],
            NOW,
            &expected(),
            ARTIFACT_SHA256,
        );
        assert_eq!(
            out.status,
            CheckStatus::TrustRootUnavailable,
            "{}",
            out.reason
        );
    }

    #[test]
    fn tampered_payload_fails() {
        let pki = build_pki(SAN_URI);
        let signed = statement(ARTIFACT_SHA256);
        let sig = sign(&pki.leaf_key, &pae(IN_TOTO_TYPE, &signed));
        // Swap the payload for a different statement while keeping the original signature.
        let tampered =
            statement("0000000000000000000000000000000000000000000000000000000000000000");
        let b = bundle(
            BUNDLE_MEDIA_TYPE_V0_3,
            &pki.leaf,
            &dsse_envelope(&tampered, &sig),
        );
        let out = verify(&b, &pki);
        assert_eq!(out.status, CheckStatus::Failed, "{}", out.reason);
    }

    #[test]
    fn signature_over_raw_payload_not_pae_fails() {
        let pki = build_pki(SAN_URI);
        let stmt = statement(ARTIFACT_SHA256);
        let raw_sig = sign(&pki.leaf_key, &stmt); // signed raw payload, not the PAE
        let b = bundle(
            BUNDLE_MEDIA_TYPE_V0_3,
            &pki.leaf,
            &dsse_envelope(&stmt, &raw_sig),
        );
        let out = verify(&b, &pki);
        assert_eq!(out.status, CheckStatus::Failed, "{}", out.reason);
    }

    #[test]
    fn subject_digest_mismatch() {
        let pki = build_pki(SAN_URI);
        // Valid identity + valid DSSE, but the statement commits to a different digest than expected.
        let stmt = statement("0000000000000000000000000000000000000000000000000000000000000000");
        let sig = sign(&pki.leaf_key, &pae(IN_TOTO_TYPE, &stmt));
        let b = bundle(
            BUNDLE_MEDIA_TYPE_V0_3,
            &pki.leaf,
            &dsse_envelope(&stmt, &sig),
        );
        let out = verify(&b, &pki);
        assert_eq!(
            out.status,
            CheckStatus::SubjectDigestMismatch,
            "{}",
            out.reason
        );
    }

    /// A bundle may carry transparency material (tlogEntries); a-3.3b ignores it and makes NO Rekor
    /// claim. The verdict is still based purely on chain + identity + DSSE + digest.
    #[test]
    fn rekor_material_present_is_ignored_no_transparency_claim() {
        let pki = build_pki(SAN_URI);
        let stmt = statement(ARTIFACT_SHA256);
        let sig = sign(&pki.leaf_key, &pae(IN_TOTO_TYPE, &stmt));
        let b = format!(
            r#"{{"mediaType":"{BUNDLE_MEDIA_TYPE_V0_3}","verificationMaterial":{{"certificate":{{"rawBytes":"{}"}},"tlogEntries":[{{"logIndex":"1","inclusionProof":{{"checkpoint":{{"envelope":"x"}}}}}}]}},"dsseEnvelope":{}}}"#,
            BASE64.encode(&pki.leaf),
            dsse_envelope(&stmt, &sig)
        );
        let out = verify(b.as_bytes(), &pki);
        // Verified on the composed primitives; the tlogEntries are neither verified nor required here.
        assert_eq!(out.status, CheckStatus::Verified, "{}", out.reason);
    }

    /// The real boundary: tlog material present does NOT compensate for a missing pinned trust root. Even
    /// with tlogEntries in the bundle, withholding the pinned root is `TrustRootUnavailable`, never
    /// `Verified` — transparency metadata is not trust material.
    #[test]
    fn tlog_present_but_root_withheld_is_trust_root_unavailable() {
        let pki = build_pki(SAN_URI);
        let stmt = statement(ARTIFACT_SHA256);
        let sig = sign(&pki.leaf_key, &pae(IN_TOTO_TYPE, &stmt));
        let b = format!(
            r#"{{"mediaType":"{BUNDLE_MEDIA_TYPE_V0_3}","verificationMaterial":{{"certificate":{{"rawBytes":"{}"}},"tlogEntries":[{{"logIndex":"1","inclusionProof":{{"checkpoint":{{"envelope":"x"}}}}}}]}},"dsseEnvelope":{}}}"#,
            BASE64.encode(&pki.leaf),
            dsse_envelope(&stmt, &sig)
        );
        let out = verify_sigstore_dsse_bundle_offline(
            b.as_bytes(),
            &[&pki.intermediate],
            &[], // pinned root withheld
            NOW,
            &expected(),
            ARTIFACT_SHA256,
        );
        assert_eq!(
            out.status,
            CheckStatus::TrustRootUnavailable,
            "{}",
            out.reason
        );
    }
}
