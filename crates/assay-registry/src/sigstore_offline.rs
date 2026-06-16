//! MCP04a-3.1 — offline Sigstore trust/cert primitive (cert-chain + validity window).
//!
//! Validates a Fulcio-style X.509 certificate chain against PINNED trust roots, entirely OFFLINE.
//! Path validation is delegated to `rustls-webpki` (we never hand-roll it); this module only maps the
//! outcome onto the supply-chain `CheckStatus` vocabulary. **No network: the verdict is produced solely
//! from the certificate bytes passed in plus the pinned root bytes — there is no client, no async, no
//! lookup.** `cert-chain-valid` is NOT `identity-authorized`: identity extraction + the Sigstore policy
//! checks (issuer/SAN, Rekor inclusion) are separate slices (a-3.1 identity, a-3.3 Rekor).
//!
//! Scope (this increment): chain validation + validity-window. Status mapping (locked in the MCP04
//! design-of-record): no pinned roots -> `TrustRootUnavailable`; chain does not validate against the
//! pinned roots -> `Failed`; expired / not-yet-valid -> `Failed`; chain valid -> `Verified`.

use rustls_pki_types::{CertificateDer, SignatureVerificationAlgorithm, UnixTime};
use webpki::{anchor_from_trusted_cert, EndEntityCert, KeyUsage};

use crate::supply_chain::CheckStatus;

/// Code-signing EKU OID `1.3.6.1.5.5.7.3.3`, encoded as the OID value bytes webpki's `KeyUsage::required`
/// expects (Fulcio leaf certificates carry this EKU).
const EKU_CODE_SIGNING: &[u8] = &[0x2b, 0x06, 0x01, 0x05, 0x05, 0x07, 0x03, 0x03];

/// ECDSA P-256 / SHA-256 — the algorithm Fulcio leaf certificates are signed with. Pinned explicitly so
/// the verifier accepts only what Sigstore actually uses, not an open set.
static SUPPORTED_SIG_ALGS: &[&dyn SignatureVerificationAlgorithm] =
    &[webpki::ring::ECDSA_P256_SHA256];

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
        Err(_) => CertChainOutcome::new(CheckStatus::Failed, "certificate chain invalid"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcgen::{
        BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
        PKCS_ECDSA_P256_SHA256,
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
    /// window, all in-memory. `offset_now` is the rcgen `OffsetDateTime` matching NOW for the window.
    fn build_pki(not_before: OffsetDateTime, not_after: OffsetDateTime) -> Pki {
        let ca_key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).unwrap();
        let mut ca_params = CertificateParams::new(Vec::<String>::new()).unwrap();
        ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        let ca_cert = ca_params.self_signed(&ca_key).unwrap();

        let leaf_key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).unwrap();
        let mut leaf_params = CertificateParams::new(vec![IDENTITY.to_string()]).unwrap();
        leaf_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::CodeSigning];
        leaf_params.not_before = not_before;
        leaf_params.not_after = not_after;
        let issuer = Issuer::from_params(&ca_params, &ca_key);
        let leaf_cert = leaf_params.signed_by(&leaf_key, &issuer).unwrap();

        Pki {
            ca_der: ca_cert.der().to_vec(),
            leaf_der: leaf_cert.der().to_vec(),
        }
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
