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
fn checkpoint_missing_root_hash_is_failed() {
    let outcome = verify(
        "rekor2-checkpoint-missing-root-hash_fail",
        TransparencyRequirement::Required,
    );
    assert_eq!(outcome.status, CheckStatus::Failed, "{}", outcome.reason);
}

#[test]
fn cosigned_checkpoint_verifies_via_pinned_log_signature() {
    // A checkpoint with multiple signatures (log + witness cosigs) verifies as long as the pinned log's
    // own signature (name == origin, hint == log id) is present and valid. Witness cosigs are ignored.
    let outcome = verify(
        "rekor2-checkpoint-two-sigs-cosigned",
        TransparencyRequirement::Required,
    );
    assert_eq!(outcome.status, CheckStatus::Verified, "{}", outcome.reason);
}

// --- DSSE-layer negative ---

#[test]
fn dsse_mismatch_signature_is_failed() {
    let outcome = verify(
        "rekor2-dsse-mismatch-sig_fail",
        TransparencyRequirement::Required,
    );
    assert_eq!(outcome.status, CheckStatus::Failed, "{}", outcome.reason);
}

// --- log-identity + cardinality + proof-math negatives (programmatic mutations of the real vector) ---

fn happy_bundle_value() -> serde_json::Value {
    serde_json::from_slice(&fixture("rekor2-happy-path", "bundle.sigstore.json")).unwrap()
}

fn verify_value(bundle: &serde_json::Value) -> RekorInclusionOutcome {
    verify_rekor_v2_inclusion_offline(
        serde_json::to_vec(bundle).unwrap().as_slice(),
        &fixture("rekor2-happy-path", "trusted_root.json"),
        TransparencyRequirement::Required,
    )
}

#[test]
fn entry_log_id_not_in_trusted_root_is_failed() {
    let mut b = happy_bundle_value();
    b["verificationMaterial"]["tlogEntries"][0]["logId"]["keyId"] =
        serde_json::json!("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=");
    let outcome = verify_value(&b);
    assert_eq!(outcome.status, CheckStatus::Failed, "{}", outcome.reason);
}

#[test]
fn multiple_tlog_entries_is_unsupported_format() {
    let mut b = happy_bundle_value();
    let entry = b["verificationMaterial"]["tlogEntries"][0].clone();
    b["verificationMaterial"]["tlogEntries"]
        .as_array_mut()
        .unwrap()
        .push(entry);
    let outcome = verify_value(&b);
    assert_eq!(
        outcome.status,
        CheckStatus::UnsupportedFormat,
        "{}",
        outcome.reason
    );
}

#[test]
fn wrong_log_index_is_failed() {
    let mut b = happy_bundle_value();
    b["verificationMaterial"]["tlogEntries"][0]["inclusionProof"]["logIndex"] =
        serde_json::json!("100");
    let outcome = verify_value(&b);
    assert_eq!(outcome.status, CheckStatus::Failed, "{}", outcome.reason);
}

#[test]
fn log_index_at_or_past_tree_size_is_failed() {
    let mut b = happy_bundle_value();
    // treeSize is 736 in the vector; logIndex == treeSize is out of range.
    b["verificationMaterial"]["tlogEntries"][0]["inclusionProof"]["logIndex"] =
        serde_json::json!("736");
    let outcome = verify_value(&b);
    assert_eq!(outcome.status, CheckStatus::Failed, "{}", outcome.reason);
}

#[test]
fn checkpoint_origin_mismatch_is_failed() {
    let mut b = happy_bundle_value();
    let env = b["verificationMaterial"]["tlogEntries"][0]["inclusionProof"]["checkpoint"]
        ["envelope"]
        .as_str()
        .unwrap()
        .to_string();
    // Rewrite the origin (first body line); this changes the signed text AND the sig-line name binding,
    // so the checkpoint signature can no longer verify under the pinned log.
    let mutated = env.replacen(
        "log2025-alpha1.rekor.sigstage.dev\n",
        "evil.example.com\n",
        1,
    );
    assert_ne!(mutated, env, "expected to rewrite the checkpoint origin");
    b["verificationMaterial"]["tlogEntries"][0]["inclusionProof"]["checkpoint"]["envelope"] =
        serde_json::json!(mutated);
    let outcome = verify_value(&b);
    assert_eq!(outcome.status, CheckStatus::Failed, "{}", outcome.reason);
}

#[test]
fn checkpoint_origin_not_matching_pinned_baseurl_is_failed() {
    // Isolate the origin<->pinned-baseUrl binding: keep a VALID checkpoint signature + key, but rewrite
    // the pinned trusted-root baseUrl host. The signature still verifies, yet the verified checkpoint is
    // for a log the operator did not pin -> Failed. (The origin-rewrite test cannot isolate this, since
    // rewriting the origin also breaks the signature.)
    let mut tr: serde_json::Value =
        serde_json::from_slice(&fixture("rekor2-happy-path", "trusted_root.json")).unwrap();
    for t in tr["tlogs"].as_array_mut().unwrap() {
        if t["publicKey"]["keyDetails"] == serde_json::json!("PKIX_ED25519") {
            t["baseUrl"] = serde_json::json!("https://other.example.com");
        }
    }
    let outcome = verify_rekor_v2_inclusion_offline(
        &fixture("rekor2-happy-path", "bundle.sigstore.json"),
        serde_json::to_vec(&tr).unwrap().as_slice(),
        TransparencyRequirement::Required,
    );
    assert_eq!(outcome.status, CheckStatus::Failed, "{}", outcome.reason);
}

#[test]
fn proof_hash_wrong_length_is_unsupported_format() {
    use base64::{engine::general_purpose::STANDARD as B64, Engine};
    let mut b = happy_bundle_value();
    // A 31-byte (wrong-length) inclusion hash is a parser/shape problem, not a crypto failure.
    b["verificationMaterial"]["tlogEntries"][0]["inclusionProof"]["hashes"][0] =
        serde_json::json!(B64.encode([0u8; 31]));
    let outcome = verify_value(&b);
    assert_eq!(
        outcome.status,
        CheckStatus::UnsupportedFormat,
        "{}",
        outcome.reason
    );
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
