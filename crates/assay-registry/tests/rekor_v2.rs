//! MCP04a-3.3c — offline Rekor v2 inclusion verifier, driven by INDEPENDENT upstream vectors
//! (sigstore-conformance @3d8491f6, Apache-2.0). The happy-path/negatives are Sigstore's own conformance
//! assets, NOT minted by this verifier — so a shared-bug false-green is excluded.

use assay_registry::rekor::{
    verify_rekor_v2_inclusion_offline, RekorInclusionOutcome, TransparencyRequirement,
};
use assay_registry::supply_chain::CheckStatus;

fn fixture(name: &str, file: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/rekor_v2/{}/{}",
        env!("CARGO_MANIFEST_DIR"),
        name,
        file
    ))
    .unwrap()
}

fn verify(name: &str, req: TransparencyRequirement) -> RekorInclusionOutcome {
    verify_rekor_v2_inclusion_offline(
        &fixture(name, "bundle.sigstore.json"),
        &fixture(name, "trusted_root.json"),
        req,
    )
}

// --- happy paths (Verified) ---

#[test]
fn rekor2_happy_path_verifies_against_pinned_trusted_root() {
    let outcome = verify("rekor2-happy-path", TransparencyRequirement::Required);
    assert_eq!(
        outcome,
        RekorInclusionOutcome {
            status: CheckStatus::Verified,
            reason: "Rekor v2 inclusion proof verifies against pinned checkpoint material",
        }
    );
}

#[test]
fn rekor2_dsse_happy_path_verifies() {
    let outcome = verify("rekor2-dsse-happy-path", TransparencyRequirement::Required);
    assert_eq!(outcome.status, CheckStatus::Verified, "{}", outcome.reason);
}

// --- requirement modes (missing proof) ---

#[test]
fn missing_required_inclusion_proof_is_online_required() {
    let outcome = verify(
        "rekor2-no-inclusion-proof_fail",
        TransparencyRequirement::Required,
    );
    assert_eq!(
        outcome.status,
        CheckStatus::OnlineRequired,
        "{}",
        outcome.reason
    );
}

#[test]
fn missing_optional_inclusion_proof_is_not_present() {
    let outcome = verify(
        "rekor2-no-inclusion-proof_fail",
        TransparencyRequirement::Optional,
    );
    assert_eq!(
        outcome.status,
        CheckStatus::NotPresent,
        "{}",
        outcome.reason
    );
}

// --- checkpoint negatives (must not verify) ---

#[test]
fn checkpoint_no_matching_signature_fails() {
    let outcome = verify(
        "rekor2-checkpoint-no-matching-signature_fail",
        TransparencyRequirement::Required,
    );
    assert_eq!(outcome.status, CheckStatus::Failed, "{}", outcome.reason);
}

#[test]
fn checkpoint_missing_root_hash_does_not_verify() {
    let outcome = verify(
        "rekor2-checkpoint-missing-root-hash_fail",
        TransparencyRequirement::Required,
    );
    assert_ne!(outcome.status, CheckStatus::Verified, "{}", outcome.reason);
}

// --- DSSE-layer negative ---

#[test]
fn dsse_mismatch_signature_does_not_verify() {
    let outcome = verify(
        "rekor2-dsse-mismatch-sig_fail",
        TransparencyRequirement::Required,
    );
    assert_ne!(outcome.status, CheckStatus::Verified, "{}", outcome.reason);
}

// --- D-LEAF=B leaf-binding negative (the anti-false-green) ---

#[test]
fn leaf_not_bound_to_bundle_cert_fails() {
    // Mutate the bundle's leaf certificate (one base64 char of its serial). The Rekor entry's
    // canonicalizedBody still embeds the ORIGINAL cert, so the D-LEAF=B bind (body cert == bundle cert)
    // breaks: a valid inclusion of a body whose cert is not THIS bundle's cert must not verify. This is
    // the "unrelated body in the log" false-green guard, on a real v2 vector.
    let bundle = String::from_utf8(fixture("rekor2-happy-path", "bundle.sigstore.json")).unwrap();
    let tampered = bundle.replacen("MIIIMTCCB7egAwIBAgIUJGo5", "MIIIMTCCB7egAwIBAgIUJGo6", 1);
    assert_ne!(tampered, bundle, "expected to mutate the bundle leaf cert");
    let outcome = verify_rekor_v2_inclusion_offline(
        tampered.as_bytes(),
        &fixture("rekor2-happy-path", "trusted_root.json"),
        TransparencyRequirement::Required,
    );
    assert_eq!(outcome.status, CheckStatus::Failed, "{}", outcome.reason);
}

// --- shape gate: Rekor v1 entry -> UnsupportedFormat ---

#[test]
fn rekor_v1_entry_is_unsupported_format() {
    let outcome = verify_rekor_v2_inclusion_offline(
        &fixture("v1-hashedrekord", "bundle.sigstore.json"),
        &fixture("rekor2-happy-path", "trusted_root.json"),
        TransparencyRequirement::Required,
    );
    assert_eq!(
        outcome.status,
        CheckStatus::UnsupportedFormat,
        "{}",
        outcome.reason
    );
}

// --- pinned material gate ---

#[test]
fn empty_trusted_root_is_trust_root_unavailable() {
    let outcome = verify_rekor_v2_inclusion_offline(
        &fixture("rekor2-happy-path", "bundle.sigstore.json"),
        br#"{"mediaType":"application/vnd.dev.sigstore.trustedroot+json;version=0.1","tlogs":[]}"#,
        TransparencyRequirement::Required,
    );
    assert_eq!(
        outcome.status,
        CheckStatus::TrustRootUnavailable,
        "{}",
        outcome.reason
    );
}

// --- mutate-the-golden-vector negative (unsigned root cannot pass) ---

#[test]
fn tampered_checkpoint_root_in_bundle_fails() {
    // Flip bytes inside the checkpoint's signed root-hash line: the checkpoint signature no longer
    // verifies, so a self-consistent but unsigned root cannot pass (D-ROOT + checkpoint-sig).
    let bundle = String::from_utf8(fixture("rekor2-happy-path", "bundle.sigstore.json")).unwrap();
    let tampered = bundle.replacen(
        "rs1YPY0ydAV0lxgfrq5pE4oRpUJwo3syeps5+eGUTDI=",
        "AAAaPY0ydAV0lxgfrq5pE4oRpUJwo3syeps5+eGUTDI=",
        1,
    );
    assert_ne!(
        tampered, bundle,
        "expected to mutate the checkpoint root in the fixture"
    );
    let outcome = verify_rekor_v2_inclusion_offline(
        tampered.as_bytes(),
        &fixture("rekor2-happy-path", "trusted_root.json"),
        TransparencyRequirement::Required,
    );
    assert_eq!(outcome.status, CheckStatus::Failed, "{}", outcome.reason);
}

// --- no-network / determinism guard ---

#[test]
fn verdict_is_pure_over_bundled_bytes_no_network() {
    // The verifier is a pure function over (bundle bytes, trusted-root bytes, requirement): no client, no
    // async, no I/O. Same inputs -> same verdict, reached entirely offline. OnlineRequired arises from a
    // MISSING embedded proof, never from a failed network attempt.
    let a = verify("rekor2-happy-path", TransparencyRequirement::Required);
    let b = verify("rekor2-happy-path", TransparencyRequirement::Required);
    assert_eq!(a, b);
    assert_eq!(a.status, CheckStatus::Verified);
    assert_eq!(
        verify(
            "rekor2-no-inclusion-proof_fail",
            TransparencyRequirement::Required
        )
        .status,
        CheckStatus::OnlineRequired
    );
}
