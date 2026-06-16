//! MCP04a-3.1b — prove the offline cert-chain + identity primitives validate a REAL Fulcio chain.
//!
//! a-3.1 was built and tested only against synthetic ECDSA P-256 chains. Real Fulcio CAs sign with
//! ECDSA P-384 / SHA-384 (the Fulcio certificate spec requires "ECDSA NIST P-384 or stronger" for CAs),
//! so the original P-256-only algorithm set silently rejected genuine Fulcio bytes. These tests use an
//! INDEPENDENT upstream vector (sigstore-conformance `rekor2-dsse-happy-path`, already vendored for the
//! a-3.3c Rekor work) so the proof is not self-minted: the leaf chains to the pinned Fulcio root through
//! a P-384 intermediate, and the pinned-identity match works on the real SAN/issuer.

use assay_registry::sigstore_identity::{verify_identity_offline, ExpectedIdentity};
use assay_registry::sigstore_offline::verify_cert_chain_offline;
use assay_registry::supply_chain::CheckStatus;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde_json::Value;

const VECTOR: &str = "rekor2-dsse-happy-path";
/// The real leaf's validity window is 2026-05-13T19:23:32Z .. 19:33:32Z; pick a `now` inside it. Cert
/// validity is what we test here — NOT timestamp freshness (that is a separate, unverified dimension).
const NOW: u64 = 1_778_700_300; // 2026-05-13T19:25:00Z

// The real Fulcio identity bound into the vector's leaf (OIDC beacon workflow + GitHub Actions issuer).
const REAL_SAN: &str = "https://github.com/sigstore-conformance/extremely-dangerous-public-oidc-beacon/.github/workflows/extremely-dangerous-oidc-beacon.yml@refs/heads/main";
const REAL_ISSUER: &str = "https://token.actions.githubusercontent.com";

fn fixture(file: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/rekor_v2/{}/{}",
        env!("CARGO_MANIFEST_DIR"),
        VECTOR,
        file
    ))
    .unwrap()
}

/// Extract `(root_der, intermediate_ders, leaf_der)` from the real bundle + trusted root. The trusted
/// root `certChain` is `[intermediate, root]`; the bundle supplies only the leaf (trust material is
/// pinned, never taken from the bundle).
fn real_chain() -> (Vec<u8>, Vec<Vec<u8>>, Vec<u8>) {
    let bundle: Value = serde_json::from_slice(&fixture("bundle.sigstore.json")).unwrap();
    let leaf = B64
        .decode(
            bundle["verificationMaterial"]["certificate"]["rawBytes"]
                .as_str()
                .unwrap(),
        )
        .unwrap();

    let tr: Value = serde_json::from_slice(&fixture("trusted_root.json")).unwrap();
    let chain = tr["certificateAuthorities"][0]["certChain"]["certificates"]
        .as_array()
        .unwrap();
    let ders: Vec<Vec<u8>> = chain
        .iter()
        .map(|c| B64.decode(c["rawBytes"].as_str().unwrap()).unwrap())
        .collect();
    let (root, inters) = ders.split_last().unwrap();
    (root.clone(), inters.to_vec(), leaf)
}

fn refs(v: &[Vec<u8>]) -> Vec<&[u8]> {
    v.iter().map(|x| x.as_slice()).collect()
}

#[test]
fn real_fulcio_p384_chain_verifies() {
    let (root, inters, leaf) = real_chain();
    let out = verify_cert_chain_offline(&leaf, &refs(&inters), &[&root], NOW);
    assert_eq!(
        out.status,
        CheckStatus::Verified,
        "real Fulcio P-384 chain must verify: {}",
        out.reason
    );
}

#[test]
fn real_fulcio_identity_matches_expected() {
    let (root, inters, leaf) = real_chain();
    let out = verify_identity_offline(
        &leaf,
        &refs(&inters),
        &[&root],
        NOW,
        &ExpectedIdentity {
            san: REAL_SAN,
            issuer: REAL_ISSUER,
        },
    );
    assert_eq!(
        out.status,
        CheckStatus::Verified,
        "real Fulcio SAN/issuer must match: {}",
        out.reason
    );
}

#[test]
fn real_fulcio_wrong_identity_is_mismatch_not_chain_failure() {
    // The chain is valid, so a wrong expected identity must be IdentityMismatch (not a chain Failed):
    // this is the pinned-identity axis a-3.4's orthogonality test depends on.
    let (root, inters, leaf) = real_chain();
    let out = verify_identity_offline(
        &leaf,
        &refs(&inters),
        &[&root],
        NOW,
        &ExpectedIdentity {
            san: "https://github.com/wrong/identity/.github/workflows/nope.yml@refs/heads/main",
            issuer: REAL_ISSUER,
        },
    );
    assert_eq!(
        out.status,
        CheckStatus::IdentityMismatch,
        "wrong SAN on a valid chain must be IdentityMismatch: {}",
        out.reason
    );
}

#[test]
fn real_fulcio_chain_without_pinned_root_is_unavailable() {
    // No pinned roots -> TrustRootUnavailable, never a silent pass and never an online fetch of the
    // Fulcio root. (Same no-network invariant as the synthetic tests, on real bytes.)
    let (_root, inters, leaf) = real_chain();
    let out = verify_cert_chain_offline(&leaf, &refs(&inters), &[], NOW);
    assert_eq!(out.status, CheckStatus::TrustRootUnavailable);
}
