use super::*;
#[test]
fn trust_basis_detects_g3_authorization_context_when_all_fields_present() {
    let bundle = make_bundle(vec![make_event(
        "assay.tool.decision",
        "run_g3",
        0,
        json!({
            "tool": "t",
            "decision": "allow",
            "principal": "alice@example.com",
            "auth_scheme": "jwt_bearer",
            "auth_issuer": "https://issuer.example/"
        }),
    )]);

    let trust_basis = generate_trust_basis(
        Cursor::new(bundle),
        VerifyLimits::default(),
        TrustBasisOptions::default(),
    )
    .expect("trust basis should generate");

    assert_eq!(
        claim(&trust_basis, TrustClaimId::AuthorizationContextVisible).level,
        TrustClaimLevel::Verified
    );
}

#[test]
fn trust_basis_g3_absent_when_principal_whitespace_only() {
    let bundle = make_bundle(vec![make_event(
        "assay.tool.decision",
        "run_g3_ws",
        0,
        json!({
            "principal": "   \n\t  ",
            "auth_scheme": "jwt_bearer",
            "auth_issuer": "https://issuer.example/"
        }),
    )]);

    let trust_basis = generate_trust_basis(
        Cursor::new(bundle),
        VerifyLimits::default(),
        TrustBasisOptions::default(),
    )
    .expect("trust basis should generate");

    assert_eq!(
        claim(&trust_basis, TrustClaimId::AuthorizationContextVisible).level,
        TrustClaimLevel::Absent
    );
}

#[test]
fn trust_basis_g3_absent_when_auth_issuer_jws_shaped_or_principal_bearer() {
    let jws = "eyJxxxxxxxxxxxxxxxxxxxx.yyyyyyyyyyyyyyyyyyyyyyyy.zzzzzzzzzzzzzzzzzzzzzzzz";
    let bundle_jws_iss = make_bundle(vec![make_event(
        "assay.tool.decision",
        "run_g3_jws_iss",
        0,
        json!({
            "principal": "alice",
            "auth_scheme": "oauth2",
            "auth_issuer": jws
        }),
    )]);
    let tb1 = generate_trust_basis(
        Cursor::new(bundle_jws_iss),
        VerifyLimits::default(),
        TrustBasisOptions::default(),
    )
    .expect("trust basis");
    assert_eq!(
        claim(&tb1, TrustClaimId::AuthorizationContextVisible).level,
        TrustClaimLevel::Absent
    );

    let bundle_bearer_princ = make_bundle(vec![make_event(
        "assay.tool.decision",
        "run_g3_bearer_p",
        0,
        json!({
            "principal": "Bearer leaked-token",
            "auth_scheme": "oauth2",
            "auth_issuer": "https://issuer.example/"
        }),
    )]);
    let tb2 = generate_trust_basis(
        Cursor::new(bundle_bearer_princ),
        VerifyLimits::default(),
        TrustBasisOptions::default(),
    )
    .expect("trust basis");
    assert_eq!(
        claim(&tb2, TrustClaimId::AuthorizationContextVisible).level,
        TrustClaimLevel::Absent
    );
}

#[test]
fn trust_basis_g3_absent_when_auth_issuer_exceeds_cap() {
    let huge_iss = "x".repeat(crate::g3_authorization_context::G3_MAX_AUTH_ISSUER_BYTES + 1);
    let bundle = make_bundle(vec![make_event(
        "assay.tool.decision",
        "run_g3_huge_iss",
        0,
        json!({
            "principal": "alice",
            "auth_scheme": "oauth2",
            "auth_issuer": huge_iss
        }),
    )]);

    let trust_basis = generate_trust_basis(
        Cursor::new(bundle),
        VerifyLimits::default(),
        TrustBasisOptions::default(),
    )
    .expect("trust basis");
    assert_eq!(
        claim(&trust_basis, TrustClaimId::AuthorizationContextVisible).level,
        TrustClaimLevel::Absent
    );
}

#[test]
fn trust_basis_keeps_signing_and_provenance_absent_despite_tempting_metadata() {
    let bundle = make_bundle(vec![make_event(
        "assay.tool.decision",
        "run_conservative",
        0,
        json!({
            "tool": "tool.commit",
            "decision": "allow",
            "signature": "pretend",
            "provenance": { "claimed": true }
        }),
    )]);

    let trust_basis = generate_trust_basis(
        Cursor::new(bundle),
        VerifyLimits::default(),
        TrustBasisOptions::default(),
    )
    .expect("trust basis should generate");

    assert_eq!(
        claim(&trust_basis, TrustClaimId::SigningEvidencePresent).level,
        TrustClaimLevel::Absent
    );
    assert_eq!(
        claim(&trust_basis, TrustClaimId::ProvenanceBackedClaimsPresent).level,
        TrustClaimLevel::Absent
    );
}

#[test]
fn trust_basis_marks_pack_findings_only_when_explicit_pack_execution_finds_results() {
    let pack = load_pack("owasp-agentic-a3-a5-signal-followup").expect("pack should load");
    let bundle = make_bundle(vec![make_event(
        "assay.tool.decision",
        "run_pack_findings",
        0,
        json!({
            "tool": "tool.commit",
            "decision": "allow",
            "principal": "user:alice"
        }),
    )]);

    let trust_basis = generate_trust_basis(
        Cursor::new(bundle),
        VerifyLimits::default(),
        TrustBasisOptions {
            lint: Some(LintOptions {
                packs: vec![pack],
                max_results: Some(500),
                bundle_path: Some("trust-basis-pack.tar.gz".to_string()),
            }),
        },
    )
    .expect("trust basis should generate");

    assert_eq!(
        claim(&trust_basis, TrustClaimId::AppliedPackFindingsPresent).level,
        TrustClaimLevel::Verified
    );
}
