//! MCP04a-3.4 — the Sigstore-keyless path of the supply-chain carrier producer, driven by the INDEPENDENT
//! upstream vector (sigstore-conformance `rekor2-dsse-happy-path`) plus surgical serde_json mutations. No
//! synthetic PKI: every dimension is exercised on real Fulcio/DSSE/Rekor-v2 bytes.
//!
//! PRODUCER-ONLY NOTE (a-3.4): this carrier adds append-only provenance dimensions (cert_chain, identity,
//! dsse_pae, timestamp_freshness, consistency, witnessing) and a new `not_checked` status. The existing
//! Plimsoll a-2 consumer is forward-tolerant but clean-by-omission for these, so this carrier is NOT
//! Plimsoll-consumable until the paired a-3.4b consumer lands; no witness/relabel may rely on the new
//! fields until then.

use assay_registry::supply_chain::{
    verify_supply_chain, CheckStatus, ContainerRef, PinningInput, Policy, ProvenanceInput,
    SigstoreBundleInput, SlsaLevel, Subject, VerifyInput,
};
use assay_registry::trust::TrustStore;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde_json::Value;

const VECTOR: &str = "rekor2-dsse-happy-path";
/// Inside the real leaf validity window (2026-05-13T19:23:32Z .. 19:33:32Z). Cert validity only.
const NOW: u64 = 1_778_700_300;
const REAL_SAN: &str = "https://github.com/sigstore-conformance/extremely-dangerous-public-oidc-beacon/.github/workflows/extremely-dangerous-oidc-beacon.yml@refs/heads/main";
const REAL_ISSUER: &str = "https://token.actions.githubusercontent.com";
/// The in-toto subject digest carried by the vector's DSSE statement (artifact `a.txt`).
const SUBJECT_DIGEST: &str =
    "sha256:a0cfc71271d6e278e57cd332ff957c3f7043fdda354c4cbb190a30d56efa01bf";

fn fixture(file: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/rekor_v2/{}/{}",
        env!("CARGO_MANIFEST_DIR"),
        VECTOR,
        file
    ))
    .unwrap()
}

/// `(roots, intermediates)` from the trusted root: `certChain` is `[intermediate, root]`.
fn fulcio_material() -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
    let tr: Value = serde_json::from_slice(&fixture("trusted_root.json")).unwrap();
    let chain = tr["certificateAuthorities"][0]["certChain"]["certificates"]
        .as_array()
        .unwrap();
    let ders: Vec<Vec<u8>> = chain
        .iter()
        .map(|c| B64.decode(c["rawBytes"].as_str().unwrap()).unwrap())
        .collect();
    let (root, inters) = ders.split_last().unwrap();
    (vec![root.clone()], inters.to_vec())
}

fn base_bundle_input(bundle_json: Vec<u8>) -> SigstoreBundleInput {
    let (roots, inters) = fulcio_material();
    SigstoreBundleInput {
        bundle_json,
        fulcio_roots: roots,
        fulcio_intermediates: inters,
        rekor_trusted_root_json: fixture("trusted_root.json"),
        now_unix_secs: NOW,
        expected_san: REAL_SAN.to_string(),
        expected_issuer: REAL_ISSUER.to_string(),
    }
}

fn subject() -> Subject {
    Subject {
        name: "mcp-pack".to_string(),
        version: "1.0.0".to_string(),
        digest: SUBJECT_DIGEST.to_string(),
    }
}

fn clean_pinning() -> PinningInput {
    PinningInput {
        version_pinned: true,
        digest_pinned: Some(true),
        lockfile_digest: Some(SUBJECT_DIGEST.to_string()),
        floating_source_ref: false,
        container_ref: Some(ContainerRef::DigestPinned),
    }
}

fn policy() -> Policy {
    Policy {
        required_builder_id: None,
        required_slsa_build_level: SlsaLevel(0), // SLSA-level is the pinned-key model, not the keyless path
        require_rekor_inclusion: false,
        require_timestamp_freshness: false,
        require_consistency: false,
        require_witnessing: false,
    }
}

fn run(
    sb: SigstoreBundleInput,
    policy: Policy,
) -> assay_registry::supply_chain::SupplyChainConformance {
    let store = TrustStore::new();
    verify_supply_chain(VerifyInput {
        subject: subject(),
        expected_artifact_digest: None,
        provenance: ProvenanceInput::SigstoreBundle(Box::new(sb)),
        pinning: clean_pinning(),
        policy,
        trust_store: &store,
    })
}

/// Parse the real bundle so a test can mutate it before re-serializing.
fn bundle_value() -> Value {
    serde_json::from_slice(&fixture("bundle.sigstore.json")).unwrap()
}

// --- Test 1: real happy path -> every dimension Verified, transparency NotChecked, Pass ---
#[test]
fn real_sigstore_dsse_happy_path_is_pass() {
    let report = run(base_bundle_input(fixture("bundle.sigstore.json")), policy());
    let p = &report.checks.provenance;
    assert_eq!(p.cert_chain, CheckStatus::Verified, "cert_chain");
    assert_eq!(p.identity, CheckStatus::Verified, "identity");
    assert_eq!(p.dsse_pae, CheckStatus::Verified, "dsse_pae");
    assert_eq!(p.rekor_inclusion, CheckStatus::Verified, "rekor_inclusion");
    assert_eq!(
        p.sigstore_bundle,
        CheckStatus::Verified,
        "sigstore_bundle shape"
    );
    assert_eq!(
        report.checks.integrity.subject_digest_binding,
        CheckStatus::Verified,
        "subject_digest_binding"
    );
    // The pinned-key fields do not apply to the keyless path.
    assert_eq!(p.dsse_signature, CheckStatus::NotApplicable);
    assert_eq!(p.slsa_provenance, CheckStatus::NotApplicable);
    assert_eq!(p.builder_identity, CheckStatus::NotApplicable);
    // Transparency extensions are deliberately not checked offline.
    assert_eq!(p.timestamp_freshness, CheckStatus::NotChecked);
    assert_eq!(p.consistency, CheckStatus::NotChecked);
    assert_eq!(p.witnessing, CheckStatus::NotChecked);
    assert_eq!(
        report.policy_result,
        assay_registry::supply_chain::PolicyResult::Pass
    );
    assert!(report
        .coverage
        .limits
        .iter()
        .any(|l| l.contains("timestamp freshness not checked")));
}

// --- Test 1b (LOAD-BEARING): valid DSSE signature, but the EVALUATED artifact != the statement subject.
// dsse_pae is signature-only, so it stays Verified (the signature over the real statement is valid); the
// mismatch isolates to subject_digest_binding. Rekor stays Verified (the bundle entry is unchanged) -- the
// mismatch is "evaluated artifact vs statement subject", not "bundle vs log". ---
#[test]
fn valid_signature_wrong_subject_isolates_to_subject_binding() {
    let wrong = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
    let store = TrustStore::new();
    let report = verify_supply_chain(VerifyInput {
        subject: Subject {
            name: "mcp-pack".to_string(),
            version: "1.0.0".to_string(),
            digest: wrong.to_string(),
        },
        expected_artifact_digest: None,
        provenance: ProvenanceInput::SigstoreBundle(Box::new(base_bundle_input(fixture(
            "bundle.sigstore.json",
        )))),
        // Pin the lockfile to the (wrong) evaluated digest so pinning is clean and the ONLY failure is
        // the subject binding.
        pinning: PinningInput {
            version_pinned: true,
            digest_pinned: Some(true),
            lockfile_digest: Some(wrong.to_string()),
            floating_source_ref: false,
            container_ref: Some(ContainerRef::DigestPinned),
        },
        policy: policy(),
        trust_store: &store,
    });
    let p = &report.checks.provenance;
    assert_eq!(
        p.dsse_pae,
        CheckStatus::Verified,
        "signature is valid over the statement; dsse_pae must not absorb the subject mismatch"
    );
    assert_eq!(
        report.checks.integrity.subject_digest_binding,
        CheckStatus::SubjectDigestMismatch,
        "the subject mismatch isolates here"
    );
    assert_eq!(
        p.rekor_inclusion,
        CheckStatus::Verified,
        "rekor entry unchanged"
    );
    assert_eq!(p.cert_chain, CheckStatus::Verified);
    assert_eq!(p.identity, CheckStatus::Verified);
    assert_eq!(
        report.policy_result,
        assay_registry::supply_chain::PolicyResult::Fail
    );
}

// --- Test 1c: payload-type-confusion guard. A non-in-toto payloadType must NOT verify, even though the
// bundle shape decomposes. dsse_pae/subject_digest_binding -> UnsupportedFormat; sigstore_bundle stays
// Verified (shape vs trust separation). ---
#[test]
fn non_in_toto_payload_type_is_unsupported_not_verified() {
    let mut b = bundle_value();
    b["dsseEnvelope"]["payloadType"] = Value::String("application/vnd.evil+json".to_string());
    let report = run(base_bundle_input(serde_json::to_vec(&b).unwrap()), policy());
    let p = &report.checks.provenance;
    assert_eq!(
        p.dsse_pae,
        CheckStatus::UnsupportedFormat,
        "a non-in-toto payloadType must not yield dsse_pae=Verified"
    );
    assert_eq!(
        report.checks.integrity.subject_digest_binding,
        CheckStatus::UnsupportedFormat,
        "no in-toto subject to bind for a non-in-toto payload"
    );
    assert_eq!(
        p.sigstore_bundle,
        CheckStatus::Verified,
        "bundle shape still decomposes; the type is an envelope-internal trust concern"
    );
    assert_eq!(
        report.policy_result,
        assay_registry::supply_chain::PolicyResult::Incomplete
    );
}

// --- Test 2 (LOAD-BEARING orthogonality): wrong identity, but Rekor + DSSE still Verified -> Fail ---
#[test]
fn wrong_identity_with_rekor_and_dsse_verified_is_fail() {
    let mut sb = base_bundle_input(fixture("bundle.sigstore.json"));
    sb.expected_san =
        "https://github.com/attacker/evil/.github/workflows/x.yml@refs/heads/main".to_string();
    let report = run(sb, policy());
    let p = &report.checks.provenance;
    assert_eq!(p.identity, CheckStatus::IdentityMismatch, "wrong identity");
    // Rekor=Verified can NOT launder a wrong identity; the other dimensions are computed independently.
    assert_eq!(
        p.rekor_inclusion,
        CheckStatus::Verified,
        "rekor still verified"
    );
    assert_eq!(p.dsse_pae, CheckStatus::Verified, "dsse still verified");
    assert_eq!(p.cert_chain, CheckStatus::Verified, "chain still verified");
    assert_eq!(
        report.policy_result,
        assay_registry::supply_chain::PolicyResult::Fail
    );
}

// --- Test 3 (accepted coupling): tampering the DSSE signature breaks BOTH dsse_pae and rekor ---
#[test]
fn tampered_dsse_signature_fails_both_dsse_and_rekor() {
    let mut b = bundle_value();
    // Flip the lowest-order byte of the ECDSA `s` value: `s` stays a valid in-range scalar (so the
    // signature is still well-formed DER and p256 accepts the encoding) but no longer verifies ->
    // dsse_pae = Failed (crypto-invalid), not UnsupportedFormat (which would mean a malformed encoding).
    let sig = b["dsseEnvelope"]["signatures"][0]["sig"].as_str().unwrap();
    let mut raw = B64.decode(sig).unwrap();
    let last = raw.len() - 1;
    raw[last] ^= 0x01;
    b["dsseEnvelope"]["signatures"][0]["sig"] = Value::String(B64.encode(&raw));
    let bundle_json = serde_json::to_vec(&b).unwrap();

    let report = run(base_bundle_input(bundle_json), policy());
    let p = &report.checks.provenance;
    assert_eq!(p.dsse_pae, CheckStatus::Failed, "dsse_pae must fail");
    // Coupling: the Rekor v2 entry binds the DSSE signature (canonicalizedBody.signature.content), so a
    // tampered DSSE signature also breaks inclusion. This is accepted coupling, not lost orthogonality.
    assert_ne!(
        p.rekor_inclusion,
        CheckStatus::Verified,
        "rekor must not stay verified once the DSSE signature is tampered"
    );
    assert_eq!(
        report.policy_result,
        assay_registry::supply_chain::PolicyResult::Fail
    );
}

// --- Test 4: timestamp freshness required -> NotChecked yields Incomplete (no magic pass) ---
#[test]
fn timestamp_freshness_required_is_incomplete() {
    let mut pol = policy();
    pol.require_timestamp_freshness = true;
    let report = run(base_bundle_input(fixture("bundle.sigstore.json")), pol);
    assert_eq!(
        report.checks.provenance.timestamp_freshness,
        CheckStatus::NotChecked
    );
    assert_eq!(
        report.policy_result,
        assay_registry::supply_chain::PolicyResult::Incomplete
    );
}

// --- Test 5: unsupported bundle shape -> sigstore_bundle + dependent dims UnsupportedFormat, Incomplete ---
#[test]
fn unsupported_bundle_shape_is_incomplete() {
    let mut b = bundle_value();
    b["mediaType"] = Value::String("application/vnd.dev.sigstore.bundle.v0.1+json".to_string());
    let report = run(base_bundle_input(serde_json::to_vec(&b).unwrap()), policy());
    let p = &report.checks.provenance;
    assert_eq!(p.sigstore_bundle, CheckStatus::UnsupportedFormat);
    assert_eq!(p.cert_chain, CheckStatus::UnsupportedFormat);
    assert_eq!(p.identity, CheckStatus::UnsupportedFormat);
    assert_eq!(p.dsse_pae, CheckStatus::UnsupportedFormat);
    assert_eq!(p.rekor_inclusion, CheckStatus::UnsupportedFormat);
    assert_eq!(
        report.policy_result,
        assay_registry::supply_chain::PolicyResult::Incomplete
    );
}

// --- Test 6: malformed bundle bytes -> sigstore_bundle Failed (blocking), Fail ---
#[test]
fn malformed_bundle_bytes_is_fail() {
    let report = run(base_bundle_input(b"{ not valid json".to_vec()), policy());
    let p = &report.checks.provenance;
    assert_eq!(p.sigstore_bundle, CheckStatus::Failed);
    assert_eq!(p.cert_chain, CheckStatus::Failed);
    assert_eq!(
        report.policy_result,
        assay_registry::supply_chain::PolicyResult::Fail
    );
}

// --- Test 7: required Rekor inclusion but proof absent -> OnlineRequired -> Incomplete ---
#[test]
fn required_rekor_absent_is_incomplete() {
    let mut b = bundle_value();
    b["verificationMaterial"]
        .as_object_mut()
        .unwrap()
        .remove("tlogEntries");
    let mut pol = policy();
    pol.require_rekor_inclusion = true;
    let report = run(base_bundle_input(serde_json::to_vec(&b).unwrap()), pol);
    let p = &report.checks.provenance;
    // Chain/identity/dsse remain verified; only inclusion is absent.
    assert_eq!(p.cert_chain, CheckStatus::Verified);
    assert_eq!(p.identity, CheckStatus::Verified);
    assert_eq!(p.dsse_pae, CheckStatus::Verified);
    assert_eq!(p.rekor_inclusion, CheckStatus::OnlineRequired);
    assert_eq!(
        report.policy_result,
        assay_registry::supply_chain::PolicyResult::Incomplete
    );
}

// --- Test 8: optional Rekor absent -> offline-first: NotPresent, still Pass with a coverage limit ---
#[test]
fn optional_rekor_absent_is_pass_offline_first() {
    let mut b = bundle_value();
    b["verificationMaterial"]
        .as_object_mut()
        .unwrap()
        .remove("tlogEntries");
    let report = run(base_bundle_input(serde_json::to_vec(&b).unwrap()), policy());
    let p = &report.checks.provenance;
    assert_eq!(p.rekor_inclusion, CheckStatus::NotPresent);
    assert_eq!(p.cert_chain, CheckStatus::Verified);
    assert_eq!(
        report.policy_result,
        assay_registry::supply_chain::PolicyResult::Pass
    );
}

// --- Contract: the carrier shape is append-only v0 with the new dimensions + not_checked serialized ---
#[test]
fn carrier_shape_is_append_only_v0_with_new_dimensions() {
    let report = run(base_bundle_input(fixture("bundle.sigstore.json")), policy());
    let v = serde_json::to_value(&report).unwrap();
    assert_eq!(v["schema"], "assay.supply_chain_conformance.v0");
    let prov = &v["checks"]["provenance"];
    // Existing v0 fields still present.
    for f in [
        "dsse_signature",
        "slsa_provenance",
        "builder_identity",
        "sigstore_bundle",
        "rekor_inclusion",
    ] {
        assert!(prov.get(f).is_some(), "missing existing field {f}");
    }
    // New append-only dimensions present, snake_case.
    for f in [
        "cert_chain",
        "identity",
        "dsse_pae",
        "timestamp_freshness",
        "consistency",
        "witnessing",
    ] {
        assert!(prov.get(f).is_some(), "missing new field {f}");
    }
    // The new status value serializes as snake_case `not_checked`.
    assert_eq!(prov["timestamp_freshness"], "not_checked");
}
