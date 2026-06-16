//! MCP04a-3.1 — offline Sigstore trust/cert primitive (cert-chain + validity window).
//!
//! Validates a Fulcio-style X.509 certificate chain against PINNED trust roots, entirely OFFLINE.
//! Path validation is delegated to `rustls-webpki` (we never hand-roll it); this module only maps the
//! outcome onto the supply-chain `CheckStatus` vocabulary. **No network: the verdict is produced solely
//! from the certificate bytes passed in plus the pinned root bytes — there is no client, no async, no
//! lookup.** `cert-chain-valid` is NOT `identity-authorized`: identity extraction + the Sigstore policy
//! checks (issuer/SAN, Rekor inclusion) are separate slices (a-3.2 identity, a-3.3 Rekor).
//!
//! Scope (this increment): chain validation + validity-window + the code-signing EKU requirement.
//! Status mapping (locked in the MCP04 design-of-record): no pinned roots -> `TrustRootUnavailable`;
//! chain does not validate against the pinned roots -> `Failed`; expired / not-yet-valid -> `Failed`;
//! leaf lacks the required code-signing EKU -> `Failed`; chain valid -> `Verified`.

use rustls_pki_types::{CertificateDer, SignatureVerificationAlgorithm, UnixTime};
use webpki::{anchor_from_trusted_cert, EndEntityCert, KeyUsage};

use crate::supply_chain::CheckStatus;

/// Code-signing EKU OID `1.3.6.1.5.5.7.3.3`, encoded as the OID value bytes webpki's `KeyUsage::required`
/// expects (Fulcio leaf certificates carry this EKU).
const EKU_CODE_SIGNING: &[u8] = &[0x2b, 0x06, 0x01, 0x05, 0x05, 0x07, 0x03, 0x03];

/// The ECDSA signature algorithms a Fulcio certificate chain is signed with. The Fulcio leaf KEY is
/// ECDSA P-256, but per the Fulcio certificate specification the CA chain (intermediate -> root) signs
/// with ECDSA NIST P-384 / SHA-384 (the spec requires "ECDSA NIST P-384 or stronger, or RSA-4096" for
/// CAs). Chain validation must therefore accept BOTH P-256/SHA-256 and P-384/SHA-384, otherwise a real
/// Fulcio chain is rejected. Pinned to exactly this set so the verifier accepts only what Sigstore
/// actually uses, not an open algorithm set (e.g. Ed25519 / RSA / P-521 are still rejected).
static SUPPORTED_SIG_ALGS: &[&dyn SignatureVerificationAlgorithm] = &[
    webpki::ring::ECDSA_P256_SHA256,
    webpki::ring::ECDSA_P384_SHA384,
];

/// The outcome of offline chain validation: a `CheckStatus` plus a value-free reason for the carrier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertChainOutcome {
    pub status: CheckStatus,
    pub reason: &'static str,
}

impl CertChainOutcome {
    fn new(status: CheckStatus, reason: &'static str) -> Self {
        Self { status, reason }
    }
}

/// Verify a leaf certificate chains to one of the PINNED roots and is within its validity window at
/// `now_unix_secs`, fully offline. `intermediates` may be empty for a directly-issued leaf.
///
/// This function performs NO I/O: it reads only its byte arguments. An empty `pinned_roots` is
/// `TrustRootUnavailable` (we hold no trust material), never a silent pass.
pub fn verify_cert_chain_offline(
    leaf_der: &[u8],
    intermediates: &[&[u8]],
    pinned_roots: &[&[u8]],
    now_unix_secs: u64,
) -> CertChainOutcome {
    if pinned_roots.is_empty() {
        return CertChainOutcome::new(CheckStatus::TrustRootUnavailable, "no pinned trust roots");
    }

    let root_ders: Vec<CertificateDer<'_>> = pinned_roots
        .iter()
        .map(|r| CertificateDer::from(*r))
        .collect();
    let mut anchors = Vec::with_capacity(root_ders.len());
    for der in &root_ders {
        match anchor_from_trusted_cert(der) {
            Ok(a) => anchors.push(a),
            Err(_) => {
                return CertChainOutcome::new(
                    CheckStatus::TrustRootUnavailable,
                    "pinned trust root is not a parseable certificate",
                )
            }
        }
    }

    let leaf = CertificateDer::from(leaf_der);
    let ee = match EndEntityCert::try_from(&leaf) {
        Ok(e) => e,
        Err(_) => return CertChainOutcome::new(CheckStatus::Failed, "malformed leaf certificate"),
    };
    let inter: Vec<CertificateDer<'_>> = intermediates
        .iter()
        .map(|i| CertificateDer::from(*i))
        .collect();
    let time = UnixTime::since_unix_epoch(std::time::Duration::from_secs(now_unix_secs));

    match ee.verify_for_usage(
        SUPPORTED_SIG_ALGS,
        &anchors,
        &inter,
        time,
        KeyUsage::required(EKU_CODE_SIGNING),
        None,
        None,
    ) {
        Ok(_) => CertChainOutcome::new(CheckStatus::Verified, "chain valid"),
        Err(webpki::Error::CertExpired { .. }) => {
            CertChainOutcome::new(CheckStatus::Failed, "certificate expired")
        }
        Err(webpki::Error::CertNotValidYet { .. }) => {
            CertChainOutcome::new(CheckStatus::Failed, "certificate not yet valid")
        }
        Err(webpki::Error::UnknownIssuer) => CertChainOutcome::new(
            CheckStatus::Failed,
            "chain does not validate against pinned roots",
        ),
        Err(webpki::Error::RequiredEkuNotFoundContext(_)) => {
            CertChainOutcome::new(CheckStatus::Failed, "required code-signing EKU absent")
        }
        Err(_) => CertChainOutcome::new(CheckStatus::Failed, "certificate chain invalid"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcgen::{
        BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose,
        IsCa, Issuer, KeyPair, KeyUsagePurpose, SignatureAlgorithm, PKCS_ECDSA_P256_SHA256,
        PKCS_ECDSA_P384_SHA384, PKCS_ED25519,
    };
    use time::{Duration, OffsetDateTime};

    const NOW: u64 = 1_750_000_000; // fixed verification time for determinism
    const IDENTITY: &str =
        "https://github.com/example/repo/.github/workflows/release.yml@refs/tags/v1";

    struct Pki {
        ca_der: Vec<u8>,
        leaf_der: Vec<u8>,
    }

    /// Build a synthetic CA + ECDSA-P256 leaf (code-signing EKU, SAN = IDENTITY) with the given validity
    /// window, all in-memory.
    fn build_pki(not_before: OffsetDateTime, not_after: OffsetDateTime) -> Pki {
        build_pki_with_eku(not_before, not_after, true)
    }

    /// As [`build_pki`], but `code_signing` controls whether the leaf carries the code-signing EKU. The
    /// `false` variant exists to prove the EKU requirement is load-bearing.
    fn build_pki_with_eku(
        not_before: OffsetDateTime,
        not_after: OffsetDateTime,
        code_signing: bool,
    ) -> Pki {
        let ca_key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).unwrap();
        let mut ca_params = CertificateParams::new(Vec::<String>::new()).unwrap();
        ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        let ca_cert = ca_params.self_signed(&ca_key).unwrap();

        let leaf_key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).unwrap();
        let mut leaf_params = CertificateParams::new(vec![IDENTITY.to_string()]).unwrap();
        if code_signing {
            leaf_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::CodeSigning];
        }
        leaf_params.not_before = not_before;
        leaf_params.not_after = not_after;
        let issuer = Issuer::from_params(&ca_params, &ca_key);
        let leaf_cert = leaf_params.signed_by(&leaf_key, &issuer).unwrap();

        Pki {
            ca_der: ca_cert.der().to_vec(),
            leaf_der: leaf_cert.der().to_vec(),
        }
    }

    /// A realistic three-tier chain `leaf -> intermediate CA -> self-signed root`, all ECDSA-P256 and
    /// valid at NOW. Roots/intermediates carry CA:TRUE + keyCertSign (the Fulcio shape); the leaf carries
    /// the code-signing EKU. Returns `(root_der, intermediate_der, leaf_der)`.
    fn build_three_tier() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        build_three_tier_alg(&PKCS_ECDSA_P256_SHA256)
    }

    /// As [`build_three_tier`], but every certificate in the chain is keyed/signed with `alg`. Real Fulcio
    /// chains sign the CA tier with ECDSA P-384/SHA-384, so the P-384 variant exercises the real-world
    /// algorithm; the Ed25519 variant proves an unsupported algorithm is still rejected.
    fn build_three_tier_alg(alg: &'static SignatureAlgorithm) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let nb = at(NOW as i64) - Duration::days(1);
        let na = at(NOW as i64) + Duration::days(1);
        let ca_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];

        // Distinct subject names so webpki can build the path unambiguously (an empty DN on both CAs
        // would make root and intermediate indistinguishable to name-based chain building).
        let dn = |cn: &str| {
            let mut d = DistinguishedName::new();
            d.push(DnType::CommonName, cn);
            d
        };

        // Self-signed root.
        let root_key = KeyPair::generate_for(alg).unwrap();
        let mut root_params = CertificateParams::new(Vec::<String>::new()).unwrap();
        root_params.distinguished_name = dn("Assay Test Root CA");
        root_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        root_params.key_usages = ca_usages.clone();
        root_params.not_before = nb;
        root_params.not_after = na;
        let root_cert = root_params.self_signed(&root_key).unwrap();

        // Intermediate CA, signed by the root. Faithful to the Fulcio profile: CA:TRUE with pathlen:0,
        // CertSign+CRLSign, AND the code-signing EKU. webpki's `Required` EKU check chains through every
        // non-anchor node, so a real Fulcio intermediate (which carries this EKU) is exactly what passes.
        let int_key = KeyPair::generate_for(alg).unwrap();
        let mut int_params = CertificateParams::new(Vec::<String>::new()).unwrap();
        int_params.distinguished_name = dn("Assay Test Intermediate CA");
        int_params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
        int_params.key_usages = ca_usages;
        int_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::CodeSigning];
        int_params.not_before = nb;
        int_params.not_after = na;
        let root_issuer = Issuer::from_params(&root_params, &root_key);
        let int_cert = int_params.signed_by(&int_key, &root_issuer).unwrap();

        // Leaf, signed by the intermediate.
        let leaf_key = KeyPair::generate_for(alg).unwrap();
        let mut leaf_params = CertificateParams::new(vec![IDENTITY.to_string()]).unwrap();
        leaf_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::CodeSigning];
        leaf_params.not_before = nb;
        leaf_params.not_after = na;
        let int_issuer = Issuer::from_params(&int_params, &int_key);
        let leaf_cert = leaf_params.signed_by(&leaf_key, &int_issuer).unwrap();

        (
            root_cert.der().to_vec(),
            int_cert.der().to_vec(),
            leaf_cert.der().to_vec(),
        )
    }

    fn at(secs: i64) -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(secs).unwrap()
    }

    fn valid_window() -> Pki {
        build_pki(
            at(NOW as i64) - Duration::days(1),
            at(NOW as i64) + Duration::days(1),
        )
    }

    #[test]
    fn valid_chain_verifies() {
        let pki = valid_window();
        let out = verify_cert_chain_offline(&pki.leaf_der, &[], &[&pki.ca_der], NOW);
        assert_eq!(out.status, CheckStatus::Verified, "{}", out.reason);
    }

    #[test]
    fn missing_trust_root_is_trust_root_unavailable() {
        let pki = valid_window();
        // No pinned roots at all -> we hold no trust material -> never a silent pass.
        let out = verify_cert_chain_offline(&pki.leaf_der, &[], &[], NOW);
        assert_eq!(out.status, CheckStatus::TrustRootUnavailable);
    }

    #[test]
    fn bad_chain_with_unrelated_root_fails() {
        let pki = valid_window();
        let other = valid_window(); // a different, valid CA that did NOT sign pki.leaf
        let out = verify_cert_chain_offline(&pki.leaf_der, &[], &[&other.ca_der], NOW);
        assert_eq!(out.status, CheckStatus::Failed, "{}", out.reason);
    }

    #[test]
    fn leaf_via_intermediate_to_pinned_root_verifies() {
        // The realistic Sigstore/Fulcio shape: leaf -> intermediate -> pinned root.
        let (root, intermediate, leaf) = build_three_tier();
        let out = verify_cert_chain_offline(&leaf, &[&intermediate], &[&root], NOW);
        assert_eq!(out.status, CheckStatus::Verified, "{}", out.reason);
    }

    #[test]
    fn p384_chain_verifies() {
        // Real Fulcio CAs sign with ECDSA P-384/SHA-384. A leaf -> intermediate -> root chain signed with
        // P-384 must validate, otherwise the verifier rejects genuine Fulcio bytes (proven separately by
        // the real-vector test in tests/fulcio_chain.rs).
        let (root, intermediate, leaf) = build_three_tier_alg(&PKCS_ECDSA_P384_SHA384);
        let out = verify_cert_chain_offline(&leaf, &[&intermediate], &[&root], NOW);
        assert_eq!(out.status, CheckStatus::Verified, "{}", out.reason);
    }

    #[test]
    fn unsupported_signature_algorithm_still_fails() {
        // Adding P-384 must NOT open the verifier to an arbitrary algorithm set: an Ed25519-signed chain
        // (not in SUPPORTED_SIG_ALGS) is still rejected.
        let (root, intermediate, leaf) = build_three_tier_alg(&PKCS_ED25519);
        let out = verify_cert_chain_offline(&leaf, &[&intermediate], &[&root], NOW);
        assert_eq!(out.status, CheckStatus::Failed, "{}", out.reason);
    }

    #[test]
    fn missing_intermediate_fails() {
        // Same chain, but the intermediate is withheld: the leaf cannot reach the pinned root, and we
        // do not fetch the missing link.
        let (root, _intermediate, leaf) = build_three_tier();
        let out = verify_cert_chain_offline(&leaf, &[], &[&root], NOW);
        assert_eq!(out.status, CheckStatus::Failed, "{}", out.reason);
    }

    #[test]
    fn wrong_intermediate_fails() {
        // An intermediate from an unrelated chain does not bridge this leaf to the pinned root.
        let (root, _intermediate, leaf) = build_three_tier();
        let (_other_root, other_int, _other_leaf) = build_three_tier();
        let out = verify_cert_chain_offline(&leaf, &[&other_int], &[&root], NOW);
        assert_eq!(out.status, CheckStatus::Failed, "{}", out.reason);
    }

    #[test]
    fn leaf_without_code_signing_eku_fails() {
        // An otherwise-valid leaf -> root chain, but the leaf lacks the code-signing EKU. The required
        // EKU is load-bearing: the verifier must reject it rather than accept any end-entity cert.
        let pki = build_pki_with_eku(
            at(NOW as i64) - Duration::days(1),
            at(NOW as i64) + Duration::days(1),
            false,
        );
        let out = verify_cert_chain_offline(&pki.leaf_der, &[], &[&pki.ca_der], NOW);
        assert_eq!(out.status, CheckStatus::Failed, "{}", out.reason);
        assert_eq!(out.reason, "required code-signing EKU absent");
    }

    #[test]
    fn expired_cert_fails() {
        let pki = build_pki(
            at(NOW as i64) - Duration::days(10),
            at(NOW as i64) - Duration::days(1),
        );
        let out = verify_cert_chain_offline(&pki.leaf_der, &[], &[&pki.ca_der], NOW);
        assert_eq!(out.status, CheckStatus::Failed);
        assert_eq!(out.reason, "certificate expired");
    }

    #[test]
    fn not_yet_valid_cert_fails() {
        let pki = build_pki(
            at(NOW as i64) + Duration::days(1),
            at(NOW as i64) + Duration::days(10),
        );
        let out = verify_cert_chain_offline(&pki.leaf_der, &[], &[&pki.ca_der], NOW);
        assert_eq!(out.status, CheckStatus::Failed);
        assert_eq!(out.reason, "certificate not yet valid");
    }

    #[test]
    fn malformed_leaf_fails() {
        let pki = valid_window();
        let out = verify_cert_chain_offline(b"not a certificate", &[], &[&pki.ca_der], NOW);
        assert_eq!(out.status, CheckStatus::Failed);
    }

    /// No-network guard (NEGATIVE): the verdict is sourced solely from the bytes passed in — the leaf,
    /// the intermediates, and the pinned roots — never from an ambient or online trust store. The proof
    /// is that the *same leaf* flips Verified <-> Failed purely on which root bytes we hand it:
    ///
    /// * correct root present       -> Verified   (succeeds entirely from bundled bytes)
    /// * only an unrelated root      -> Failed     (the real issuer is NOT discovered/fetched)
    /// * no roots at all             -> TrustRootUnavailable (no online fallback, no silent pass)
    ///
    /// A client that consulted the network (e.g. fetched the Fulcio root from TUF) could "rescue" the
    /// unrelated-root and no-root cases into Verified. Because we never do, those cases must fail. The
    /// trust anchor is exactly the bytes the caller pins — a pure function of its inputs.
    #[test]
    fn verdict_is_produced_only_from_bundled_bytes() {
        let pki = valid_window();
        let unrelated = valid_window(); // a different, valid CA that did NOT sign pki.leaf

        // Determinism: identical inputs -> identical verdict, no hidden state or wall clock
        // (verification time is an explicit argument).
        let a = verify_cert_chain_offline(&pki.leaf_der, &[], &[&pki.ca_der], NOW);
        let b = verify_cert_chain_offline(&pki.leaf_der, &[], &[&pki.ca_der], NOW);
        assert_eq!(a, b);
        assert_eq!(a.status, CheckStatus::Verified, "{}", a.reason);

        // Same leaf, only the pinned-root set differs -> verdict flips. The trust decision comes from
        // the passed-in roots, not from anywhere the process could reach.
        let with_wrong_root =
            verify_cert_chain_offline(&pki.leaf_der, &[], &[&unrelated.ca_der], NOW);
        assert_eq!(
            with_wrong_root.status,
            CheckStatus::Failed,
            "withholding the real root must fail, not trigger an online lookup: {}",
            with_wrong_root.reason
        );

        // No roots at all -> we hold no trust material -> unavailable, never an online attempt or a
        // silent pass.
        assert_eq!(
            verify_cert_chain_offline(&pki.leaf_der, &[], &[], NOW).status,
            CheckStatus::TrustRootUnavailable
        );
    }
}
