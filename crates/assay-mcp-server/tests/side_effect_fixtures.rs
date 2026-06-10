//! Ea guard: the side-effect receipt reference fixtures must hold the honesty-ladder invariants
//! (docs/reference/side-effect-receipt.md) and the binding must be reproducible from committed bytes
//! by the canonical JCS digest the future verifier (Eb) will use. There is no producer/verifier yet;
//! this keeps the vectors honest and proves the binding math is sound.

use assay_core::mcp::jcs;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;

fn fx(name: &str) -> Value {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/side_effect")
        .join(name);
    serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap()
}

fn binding_of(subject: &Value) -> String {
    let bytes = jcs::to_vec(subject).expect("jcs");
    format!("sha256:{}", hex::encode(Sha256::digest(&bytes)))
}

fn decision0(v: &Value) -> Value {
    v["observed_tool_decisions"][0].clone()
}

#[test]
fn audit_record_binding_recomputes_from_committed_bytes() {
    // The binding the verifier will recompute (jcs over the canonical subject) equals the committed
    // binding_digest. If this drifts, the verifier and the fixtures disagree.
    let rec = fx("audit_record_github_deploy_key.json");
    assert_eq!(
        binding_of(&rec["subject"]),
        rec["binding_digest"].as_str().unwrap(),
        "audit record binding_digest must recompute from its subject via canonical JCS"
    );
}

#[test]
fn verified_level_is_bound_to_the_matching_audit_record() {
    let verified = decision0(&fx("verified.json"));
    let rec = fx("audit_record_github_deploy_key.json");
    let se = &verified["response"]["side_effect"];
    assert_eq!(se["level"], "verified");
    assert_eq!(se["verification_source"], "provider_audit_import");
    // The verified decision's subject digest is exactly the audit record's binding.
    assert_eq!(
        se["verification_subject_digest"].as_str().unwrap(),
        rec["binding_digest"].as_str().unwrap(),
        "a verified side effect must carry the binding of the imported audit record"
    );
    // And side_effect_verified (the compat boolean) agrees with the level.
    assert_eq!(verified["response"]["side_effect_verified"], true);
}

#[test]
fn mismatched_audit_record_does_not_bind() {
    // An audit record for a different target produces a different binding, so it could never promote
    // the verified.json decision: its binding must not equal the verified decision's digest.
    let mm = fx("audit_record_mismatch.json");
    let verified = decision0(&fx("verified.json"));
    assert_eq!(
        binding_of(&mm["subject"]),
        mm["binding_digest"].as_str().unwrap()
    );
    assert_ne!(
        mm["binding_digest"].as_str().unwrap(),
        verified["response"]["side_effect"]["verification_subject_digest"]
            .as_str()
            .unwrap(),
        "a mismatched audit record must not share the verified call's binding"
    );
}

#[test]
fn asserted_never_claims_verified() {
    let a = decision0(&fx("asserted.json"));
    let se = &a["response"]["side_effect"];
    assert_eq!(se["level"], "asserted");
    assert!(se["verification_source"].is_null());
    assert!(se["verification_subject_digest"].is_null());
    // asserted is not verified: the compat boolean must stay false.
    assert_eq!(a["response"]["side_effect_verified"], false);
}

#[test]
fn observed_confirmed_is_sequence_not_audit() {
    let oc = decision0(&fx("observed_confirmed.json"));
    let se = &oc["response"]["side_effect"];
    assert_eq!(se["level"], "observed_confirmed");
    assert_eq!(se["verification_source"], "observed_read_followup");
    // observed_confirmed is sequence evidence within the run, NOT external verification.
    assert_eq!(oc["response"]["side_effect_verified"], false);
}

#[test]
fn levels_are_from_the_pinned_set() {
    let known = ["asserted", "observed_confirmed", "verified"];
    for name in ["asserted.json", "observed_confirmed.json", "verified.json"] {
        let level = decision0(&fx(name))["response"]["side_effect"]["level"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(
            known.contains(&level.as_str()),
            "{name}: unknown level {level}"
        );
    }
}
