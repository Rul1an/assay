//! MCP04a-3.2a — offline Sigstore identity extraction + expected-identity match.
//!
//! Builds on the a-3.1 cert-chain primitive. Once the leaf chains to a pinned root, this layer reads the
//! identity facts the Fulcio profile binds into the leaf — the single Subject Alternative Name (the
//! workflow / signer identity) and the OIDC issuer (Fulcio issuer **v2** extension) — and compares them
//! against the caller's EXPECTED identity. Still fully OFFLINE: it parses and compares the leaf bytes,
//! with no network, no Rekor, and no signature / subject-digest binding (that is a-3.2b).
//!
//! **cert-chain-valid is NOT identity-authorized.** A leaf that chains cleanly but carries a different
//! SAN / issuer than expected is `IdentityMismatch`, never `Verified`. The chain is a prerequisite, not a
//! sufficient condition — so this function runs the chain check first and only looks at identity once the
//! chain is `Verified`.
//!
//! Status mapping (locked in the MCP04 design-of-record):
//! - chain does not validate -> the chain status is returned verbatim (`Failed` / `TrustRootUnavailable`)
//! - non-empty subject -> `Failed` (Fulcio issued certs MUST have an empty subject; identity lives in SAN)
//! - SAN not marked critical (required on an empty-subject cert) -> `Failed`
//! - not exactly one SAN, or an unsupported SAN type -> `UnsupportedFormat`
//! - only the deprecated v1 issuer extension present -> `UnsupportedFormat`
//! - SAN or issuer absent, or different from expected -> `IdentityMismatch`
//! - SAN and issuer both match -> `Verified`

use x509_cert::der::asn1::Utf8StringRef;
use x509_cert::der::oid::{AssociatedOid, ObjectIdentifier};
use x509_cert::der::Decode;
use x509_cert::ext::pkix::name::GeneralName;
use x509_cert::ext::pkix::SubjectAltName;
use x509_cert::Certificate;

use crate::sigstore_offline::verify_cert_chain_offline;
use crate::supply_chain::CheckStatus;

/// Fulcio issuer **v2** extension (`1.3.6.1.4.1.57264.1.8`): the OIDC token issuer as a DER-encoded
/// string. This is the form we accept.
const FULCIO_ISSUER_V2: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.3.6.1.4.1.57264.1.8");
/// Fulcio issuer **v1** extension (`1.3.6.1.4.1.57264.1.1`): the deprecated raw-value form. Present only
/// so we can report `UnsupportedFormat` rather than silently treating a v1-only cert as missing identity.
const FULCIO_ISSUER_V1: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.3.6.1.4.1.57264.1.1");

/// The identity a caller expects the signing certificate to bind to. Both fields are matched exactly;
/// this slice performs no pattern/policy authorization beyond exact comparison.
#[derive(Debug, Clone, Copy)]
pub struct ExpectedIdentity<'a> {
    /// Expected Subject Alternative Name value — a URI or email (the Fulcio workflow / signer identity).
    pub san: &'a str,
    /// Expected OIDC issuer (the Fulcio issuer v2 extension value).
    pub issuer: &'a str,
}

/// The outcome of offline identity verification: a `CheckStatus` plus a value-free reason for the carrier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityOutcome {
    pub status: CheckStatus,
    pub reason: &'static str,
}

impl IdentityOutcome {
    fn new(status: CheckStatus, reason: &'static str) -> Self {
        Self { status, reason }
    }
}

/// Verify that a leaf certificate chains to a pinned root (a-3.1) AND binds to the `expected` identity,
/// fully offline. The chain is checked first; identity is only examined once the chain is `Verified`.
///
/// Reads only its byte arguments — no network, no Rekor, no signature/subject-digest binding.
pub fn verify_identity_offline(
    leaf_der: &[u8],
    intermediates: &[&[u8]],
    pinned_roots: &[&[u8]],
    now_unix_secs: u64,
    expected: &ExpectedIdentity<'_>,
) -> IdentityOutcome {
    // The chain is a prerequisite. If it does not validate, identity is not even considered: a forged or
    // untrusted cert's SAN means nothing.
    let chain = verify_cert_chain_offline(leaf_der, intermediates, pinned_roots, now_unix_secs);
    if chain.status != CheckStatus::Verified {
        return IdentityOutcome::new(chain.status, chain.reason);
    }

    let cert = match Certificate::from_der(leaf_der) {
        Ok(c) => c,
        Err(_) => return IdentityOutcome::new(CheckStatus::Failed, "malformed leaf certificate"),
    };
    let tbs = &cert.tbs_certificate;

    // Fulcio profile: issued certs MUST have an empty subject; the identity lives in the SAN.
    if !tbs.subject.0.is_empty() {
        return IdentityOutcome::new(CheckStatus::Failed, "certificate has a non-empty subject");
    }

    let extensions = match &tbs.extensions {
        Some(exts) => exts.as_slice(),
        None => {
            return IdentityOutcome::new(CheckStatus::IdentityMismatch, "no certificate extensions")
        }
    };

    // --- Subject Alternative Name (the signer identity) ---
    let san_ext = match extensions.iter().find(|e| e.extn_id == SubjectAltName::OID) {
        Some(e) => e,
        None => {
            return IdentityOutcome::new(
                CheckStatus::IdentityMismatch,
                "no subject alternative name",
            )
        }
    };
    // RFC 5280 / Fulcio: a leaf whose subject is empty (already enforced above) MUST carry the SAN as a
    // critical extension. A non-critical SAN on an empty-subject cert is a profile violation, not merely
    // an unsupported shape.
    if !san_ext.critical {
        return IdentityOutcome::new(
            CheckStatus::Failed,
            "subject alternative name is not marked critical",
        );
    }
    let san = match SubjectAltName::from_der(san_ext.extn_value.as_bytes()) {
        Ok(s) => s,
        Err(_) => {
            return IdentityOutcome::new(CheckStatus::Failed, "malformed subject alternative name")
        }
    };
    // Fulcio profile: exactly one SAN.
    if san.0.len() != 1 {
        return IdentityOutcome::new(
            CheckStatus::UnsupportedFormat,
            "certificate does not carry exactly one SAN",
        );
    }
    let san_value = match &san.0[0] {
        GeneralName::UniformResourceIdentifier(uri) => uri.as_str(),
        GeneralName::Rfc822Name(email) => email.as_str(),
        _ => return IdentityOutcome::new(CheckStatus::UnsupportedFormat, "unsupported SAN type"),
    };
    if san_value != expected.san {
        return IdentityOutcome::new(
            CheckStatus::IdentityMismatch,
            "SAN does not match expected identity",
        );
    }

    // --- OIDC issuer (Fulcio v2 extension) ---
    let issuer_ext = match extensions.iter().find(|e| e.extn_id == FULCIO_ISSUER_V2) {
        Some(e) => e,
        None => {
            if extensions.iter().any(|e| e.extn_id == FULCIO_ISSUER_V1) {
                return IdentityOutcome::new(
                    CheckStatus::UnsupportedFormat,
                    "only the deprecated v1 issuer extension is present",
                );
            }
            return IdentityOutcome::new(CheckStatus::IdentityMismatch, "no issuer extension");
        }
    };
    let issuer_value = match Utf8StringRef::from_der(issuer_ext.extn_value.as_bytes()) {
        Ok(s) => s.as_str(),
        Err(_) => {
            return IdentityOutcome::new(
                CheckStatus::UnsupportedFormat,
                "issuer extension is not a DER-encoded string",
            )
        }
    };
    if issuer_value != expected.issuer {
        return IdentityOutcome::new(
            CheckStatus::IdentityMismatch,
            "issuer does not match expected identity",
        );
    }

    IdentityOutcome::new(
        CheckStatus::Verified,
        "chain valid and identity matches expected SAN and issuer",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcgen::{
        BasicConstraints, CertificateParams, CustomExtension, DistinguishedName, DnType,
        ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair, KeyUsagePurpose, SanType,
        PKCS_ECDSA_P256_SHA256,
    };
    use time::{Duration, OffsetDateTime};

    use crate::sigstore_offline::verify_cert_chain_offline;

    const NOW: u64 = 1_750_000_000;
    const SAN_URI: &str =
        "https://github.com/example/repo/.github/workflows/release.yml@refs/tags/v1";
    const ISSUER: &str = "https://token.actions.githubusercontent.com";
    const FULCIO_V2_OID: &[u64] = &[1, 3, 6, 1, 4, 1, 57264, 1, 8];
    const FULCIO_V1_OID: &[u64] = &[1, 3, 6, 1, 4, 1, 57264, 1, 1];

    fn at(secs: i64) -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(secs).unwrap()
    }

    fn uri_san(s: &str) -> SanType {
        SanType::URI(s.try_into().unwrap())
    }

    fn expected() -> ExpectedIdentity<'static> {
        ExpectedIdentity {
            san: SAN_URI,
            issuer: ISSUER,
        }
    }

    /// DER-encode a short UTF8String (tag 0x0C, short-form length). Matches the Fulcio v2 issuer shape
    /// (the OIDC issuer as a DER-encoded string). Sufficient for the issuer values used in tests.
    fn der_utf8(s: &str) -> Vec<u8> {
        let b = s.as_bytes();
        assert!(b.len() < 128, "test issuer too long for short-form length");
        let mut v = vec![0x0c, b.len() as u8];
        v.extend_from_slice(b);
        v
    }

    const SAN_OID: &[u64] = &[2, 5, 29, 17];
    const EMAIL_SAN: &str = "signer@example.com";

    /// DER-encode a GeneralNames SEQUENCE holding a single URI (`[6]` IA5String, IMPLICIT). Short-form
    /// length only — used to inject a SAN as a (non-critical) custom extension, which rcgen will not do
    /// on an empty-subject leaf (it forces SAN-critical there).
    fn der_general_names_uri(uri: &str) -> Vec<u8> {
        let b = uri.as_bytes();
        assert!(b.len() < 126, "test uri too long for short-form length");
        let mut inner = vec![0x86, b.len() as u8];
        inner.extend_from_slice(b);
        let mut out = vec![0x30, inner.len() as u8];
        out.extend_from_slice(&inner);
        out
    }

    /// How to shape the leaf's identity fields for a given test.
    struct LeafSpec {
        sans: Vec<SanType>,
        /// `(oid, issuer-value)`; the value is DER-UTF8-encoded into the extension content.
        issuer_ext: Option<(&'static [u64], &'static str)>,
        /// Raw v2-issuer extension content, bypassing `der_utf8` (for malformed-content tests). When
        /// `Some`, it is used instead of `issuer_ext`.
        issuer_raw: Option<Vec<u8>>,
        /// Inject the SAN as a NON-critical custom extension over `SAN_URI` (rcgen forces SAN-critical on
        /// an empty subject, so this is the only way to model that profile violation). When true, `sans`
        /// is ignored.
        san_noncritical: bool,
        empty_subject: bool,
    }

    impl LeafSpec {
        /// A well-formed Fulcio-shaped leaf: one URI SAN, v2 issuer extension, empty subject.
        fn fulcio() -> Self {
            Self {
                sans: vec![uri_san(SAN_URI)],
                issuer_ext: Some((FULCIO_V2_OID, ISSUER)),
                issuer_raw: None,
                san_noncritical: false,
                empty_subject: true,
            }
        }
    }

    /// Build a root CA + a leaf signed by it from `spec`, all ECDSA-P256 and valid at NOW. The leaf
    /// always carries the code-signing EKU so the a-3.1 chain check passes; identity shape varies per
    /// spec. Returns `(root_der, leaf_der)`.
    fn build(spec: &LeafSpec) -> (Vec<u8>, Vec<u8>) {
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
        // rcgen defaults the subject to a CN; Fulcio leaves have an empty subject, so clear it unless the
        // test specifically wants a (profile-violating) non-empty subject.
        if spec.empty_subject {
            leaf_params.distinguished_name = DistinguishedName::new();
        }
        if spec.san_noncritical {
            // Bypass rcgen's SAN handling (which marks an empty-subject SAN critical) and inject the SAN
            // as a non-critical custom extension over the same OID.
            leaf_params
                .custom_extensions
                .push(CustomExtension::from_oid_content(
                    SAN_OID,
                    der_general_names_uri(SAN_URI),
                ));
        } else {
            leaf_params.subject_alt_names = spec.sans.clone();
        }
        leaf_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::CodeSigning];
        if let Some(raw) = &spec.issuer_raw {
            leaf_params
                .custom_extensions
                .push(CustomExtension::from_oid_content(
                    FULCIO_V2_OID,
                    raw.clone(),
                ));
        } else if let Some((oid, value)) = spec.issuer_ext {
            leaf_params
                .custom_extensions
                .push(CustomExtension::from_oid_content(oid, der_utf8(value)));
        }
        leaf_params.not_before = nb;
        leaf_params.not_after = na;
        let issuer = Issuer::from_params(&root_params, &root_key);
        let leaf_cert = leaf_params.signed_by(&leaf_key, &issuer).unwrap();

        (root_cert.der().to_vec(), leaf_cert.der().to_vec())
    }

    #[test]
    fn matching_san_and_issuer_verifies() {
        let (root, leaf) = build(&LeafSpec::fulcio());
        let out = verify_identity_offline(&leaf, &[], &[&root], NOW, &expected());
        assert_eq!(out.status, CheckStatus::Verified, "{}", out.reason);
    }

    #[test]
    fn wrong_san_is_identity_mismatch() {
        let mut spec = LeafSpec::fulcio();
        spec.sans = vec![uri_san(
            "https://github.com/evil/repo/.github/workflows/x.yml@refs/heads/main",
        )];
        let (root, leaf) = build(&spec);
        let out = verify_identity_offline(&leaf, &[], &[&root], NOW, &expected());
        assert_eq!(out.status, CheckStatus::IdentityMismatch, "{}", out.reason);
    }

    #[test]
    fn absent_san_is_identity_mismatch() {
        let mut spec = LeafSpec::fulcio();
        spec.sans = vec![]; // no SAN extension emitted at all
        let (root, leaf) = build(&spec);
        let out = verify_identity_offline(&leaf, &[], &[&root], NOW, &expected());
        assert_eq!(out.status, CheckStatus::IdentityMismatch, "{}", out.reason);
    }

    #[test]
    fn wrong_issuer_is_identity_mismatch() {
        let mut spec = LeafSpec::fulcio();
        spec.issuer_ext = Some((FULCIO_V2_OID, "https://accounts.google.com"));
        let (root, leaf) = build(&spec);
        let out = verify_identity_offline(&leaf, &[], &[&root], NOW, &expected());
        assert_eq!(out.status, CheckStatus::IdentityMismatch, "{}", out.reason);
    }

    #[test]
    fn absent_issuer_is_identity_mismatch() {
        let mut spec = LeafSpec::fulcio();
        spec.issuer_ext = None;
        let (root, leaf) = build(&spec);
        let out = verify_identity_offline(&leaf, &[], &[&root], NOW, &expected());
        assert_eq!(out.status, CheckStatus::IdentityMismatch, "{}", out.reason);
    }

    #[test]
    fn v1_only_issuer_is_unsupported_format() {
        let mut spec = LeafSpec::fulcio();
        spec.issuer_ext = Some((FULCIO_V1_OID, ISSUER));
        let (root, leaf) = build(&spec);
        let out = verify_identity_offline(&leaf, &[], &[&root], NOW, &expected());
        assert_eq!(out.status, CheckStatus::UnsupportedFormat, "{}", out.reason);
    }

    #[test]
    fn multiple_sans_is_unsupported_format() {
        let mut spec = LeafSpec::fulcio();
        spec.sans = vec![
            uri_san(SAN_URI),
            uri_san("https://github.com/example/repo/second"),
        ];
        let (root, leaf) = build(&spec);
        let out = verify_identity_offline(&leaf, &[], &[&root], NOW, &expected());
        assert_eq!(out.status, CheckStatus::UnsupportedFormat, "{}", out.reason);
    }

    #[test]
    fn unsupported_san_type_is_unsupported_format() {
        let mut spec = LeafSpec::fulcio();
        spec.sans = vec![SanType::DnsName("example.com".try_into().unwrap())];
        let (root, leaf) = build(&spec);
        let out = verify_identity_offline(&leaf, &[], &[&root], NOW, &expected());
        assert_eq!(out.status, CheckStatus::UnsupportedFormat, "{}", out.reason);
    }

    #[test]
    fn nonempty_subject_fails() {
        let mut spec = LeafSpec::fulcio();
        spec.empty_subject = false; // keep rcgen's default CN -> non-empty subject (profile violation)
        let (root, leaf) = build(&spec);
        let out = verify_identity_offline(&leaf, &[], &[&root], NOW, &expected());
        assert_eq!(out.status, CheckStatus::Failed, "{}", out.reason);
        assert_eq!(out.reason, "certificate has a non-empty subject");
    }

    #[test]
    fn email_san_matches_verifies() {
        // The supported-but-previously-untested email (Rfc822Name) SAN branch.
        let mut spec = LeafSpec::fulcio();
        spec.sans = vec![SanType::Rfc822Name(EMAIL_SAN.try_into().unwrap())];
        let (root, leaf) = build(&spec);
        let expected = ExpectedIdentity {
            san: EMAIL_SAN,
            issuer: ISSUER,
        };
        let out = verify_identity_offline(&leaf, &[], &[&root], NOW, &expected);
        assert_eq!(out.status, CheckStatus::Verified, "{}", out.reason);
    }

    #[test]
    fn malformed_issuer_extension_is_unsupported_format() {
        // The v2 issuer extension is present but its content is not a DER-encoded string.
        let mut spec = LeafSpec::fulcio();
        spec.issuer_raw = Some(vec![0xff, 0x02, 0x01, 0x02]);
        let (root, leaf) = build(&spec);
        let out = verify_identity_offline(&leaf, &[], &[&root], NOW, &expected());
        assert_eq!(out.status, CheckStatus::UnsupportedFormat, "{}", out.reason);
    }

    #[test]
    fn noncritical_san_on_empty_subject_fails() {
        // RFC 5280 / Fulcio: an empty-subject leaf MUST carry the SAN as a critical extension.
        let mut spec = LeafSpec::fulcio();
        spec.san_noncritical = true;
        let (root, leaf) = build(&spec);
        let out = verify_identity_offline(&leaf, &[], &[&root], NOW, &expected());
        assert_eq!(out.status, CheckStatus::Failed, "{}", out.reason);
        assert_eq!(
            out.reason,
            "subject alternative name is not marked critical"
        );
    }

    /// The headline non-claim: a leaf that chains cleanly to the pinned root but carries the WRONG
    /// identity is `IdentityMismatch`, never `Verified`. cert-chain-valid != identity-authorized.
    #[test]
    fn valid_chain_wrong_identity_is_not_verified() {
        let mut spec = LeafSpec::fulcio();
        spec.sans = vec![uri_san(
            "https://github.com/evil/repo/.github/workflows/x.yml@refs/heads/main",
        )];
        let (root, leaf) = build(&spec);
        // The chain itself is genuinely valid...
        assert_eq!(
            verify_cert_chain_offline(&leaf, &[], &[&root], NOW).status,
            CheckStatus::Verified,
        );
        // ...yet the identity layer refuses it.
        let out = verify_identity_offline(&leaf, &[], &[&root], NOW, &expected());
        assert_eq!(out.status, CheckStatus::IdentityMismatch, "{}", out.reason);
    }

    /// Identity is only examined once the chain is valid: a correct identity over an UNTRUSTED chain
    /// returns the chain failure, not `Verified`.
    #[test]
    fn correct_identity_bad_chain_fails() {
        let (_root, leaf) = build(&LeafSpec::fulcio());
        let (other_root, _other_leaf) = build(&LeafSpec::fulcio()); // unrelated CA
        let out = verify_identity_offline(&leaf, &[], &[&other_root], NOW, &expected());
        assert_eq!(out.status, CheckStatus::Failed, "{}", out.reason);
    }

    #[test]
    fn no_pinned_roots_is_trust_root_unavailable() {
        let (_root, leaf) = build(&LeafSpec::fulcio());
        let out = verify_identity_offline(&leaf, &[], &[], NOW, &expected());
        assert_eq!(
            out.status,
            CheckStatus::TrustRootUnavailable,
            "{}",
            out.reason
        );
    }
}
