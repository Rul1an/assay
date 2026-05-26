use super::*;
#[test]
fn g3_authorization_claim_is_after_delegation_before_containment() {
    let bundle = make_bundle(vec![make_event(
        "assay.process.exec",
        "run_g3_order",
        0,
        json!({ "hits": 1 }),
    )]);
    let trust_basis = generate_trust_basis(
        Cursor::new(bundle),
        VerifyLimits::default(),
        TrustBasisOptions::default(),
    )
    .expect("trust basis");
    let ids: Vec<_> = trust_basis.claims.iter().map(|c| c.id).collect();
    let pos = |id| ids.iter().position(|&x| x == id).expect("claim id");
    assert_eq!(ids.len(), 10);
    assert!(
        pos(TrustClaimId::DelegationContextVisible)
            < pos(TrustClaimId::AuthorizationContextVisible)
    );
    assert!(
        pos(TrustClaimId::AuthorizationContextVisible)
            < pos(TrustClaimId::ContainmentDegradationObserved)
    );
}

#[test]
fn trust_basis_always_emits_all_frozen_claims() {
    let bundle = make_bundle(vec![make_event(
        "assay.process.exec",
        "run_all_claims",
        0,
        json!({ "hits": 1 }),
    )]);

    let trust_basis = generate_trust_basis(
        Cursor::new(bundle),
        VerifyLimits::default(),
        TrustBasisOptions::default(),
    )
    .expect("trust basis should generate");

    assert_eq!(
        trust_basis
            .claims
            .iter()
            .map(|claim| (claim.id, claim.source, claim.boundary))
            .collect::<Vec<_>>(),
        vec![
            (
                TrustClaimId::BundleVerified,
                TrustClaimSource::BundleVerification,
                TrustClaimBoundary::BundleWide,
            ),
            (
                TrustClaimId::SigningEvidencePresent,
                TrustClaimSource::BundleProofSurface,
                TrustClaimBoundary::ProofSurfacesOnly,
            ),
            (
                TrustClaimId::ProvenanceBackedClaimsPresent,
                TrustClaimSource::BundleProofSurface,
                TrustClaimBoundary::ProofSurfacesOnly,
            ),
            (
                TrustClaimId::DelegationContextVisible,
                TrustClaimSource::CanonicalDecisionEvidence,
                TrustClaimBoundary::SupportedDelegatedFlowsOnly,
            ),
            (
                TrustClaimId::AuthorizationContextVisible,
                TrustClaimSource::CanonicalDecisionEvidence,
                TrustClaimBoundary::SupportedAuthProjectedFlowsOnly,
            ),
            (
                TrustClaimId::ContainmentDegradationObserved,
                TrustClaimSource::CanonicalEventPresence,
                TrustClaimBoundary::SupportedContainmentFallbackPathsOnly,
            ),
            (
                TrustClaimId::ExternalEvalReceiptBoundaryVisible,
                TrustClaimSource::ExternalEvidenceReceipt,
                TrustClaimBoundary::SupportedExternalEvalReceiptEventsOnly,
            ),
            (
                TrustClaimId::ExternalDecisionReceiptBoundaryVisible,
                TrustClaimSource::ExternalDecisionReceipt,
                TrustClaimBoundary::SupportedExternalDecisionReceiptEventsOnly,
            ),
            (
                TrustClaimId::ExternalInventoryReceiptBoundaryVisible,
                TrustClaimSource::ExternalInventoryReceipt,
                TrustClaimBoundary::SupportedExternalInventoryReceiptEventsOnly,
            ),
            (
                TrustClaimId::AppliedPackFindingsPresent,
                TrustClaimSource::PackExecutionResults,
                TrustClaimBoundary::PackExecutionOnly,
            ),
        ]
    );
    assert_eq!(
        trust_basis
            .claims
            .iter()
            .map(|claim| claim.level)
            .collect::<Vec<_>>(),
        vec![
            TrustClaimLevel::Verified,
            TrustClaimLevel::Absent,
            TrustClaimLevel::Absent,
            TrustClaimLevel::Absent,
            TrustClaimLevel::Absent,
            TrustClaimLevel::Absent,
            TrustClaimLevel::Absent,
            TrustClaimLevel::Absent,
            TrustClaimLevel::Absent,
            TrustClaimLevel::Absent,
        ]
    );
}

#[test]
fn trust_basis_contract_generated_claim_id_order_is_frozen() {
    let bundle = make_bundle(vec![make_event(
        "assay.process.exec",
        "run_contract_claim_order",
        0,
        json!({ "hits": 1 }),
    )]);

    let trust_basis = generate_trust_basis(
        Cursor::new(bundle),
        VerifyLimits::default(),
        TrustBasisOptions::default(),
    )
    .expect("trust basis should generate");

    let claim_ids = trust_basis
        .claims
        .iter()
        .map(|claim| serde_json::to_value(claim.id).expect("claim id serializes"))
        .collect::<Vec<_>>();

    assert_eq!(
        claim_ids,
        vec![
            json!("bundle_verified"),
            json!("signing_evidence_present"),
            json!("provenance_backed_claims_present"),
            json!("delegation_context_visible"),
            json!("authorization_context_visible"),
            json!("containment_degradation_observed"),
            json!("external_eval_receipt_boundary_visible"),
            json!("external_decision_receipt_boundary_visible"),
            json!("external_inventory_receipt_boundary_visible"),
            json!("applied_pack_findings_present"),
        ]
    );
}

#[test]
fn trust_basis_contract_canonical_json_shape_is_frozen() {
    let trust_basis = TrustBasis {
        claims: vec![
            TrustBasisClaim {
                id: TrustClaimId::BundleVerified,
                level: TrustClaimLevel::Verified,
                source: TrustClaimSource::BundleVerification,
                boundary: TrustClaimBoundary::BundleWide,
                note: None,
            },
            TrustBasisClaim {
                id: TrustClaimId::DelegationContextVisible,
                level: TrustClaimLevel::Absent,
                source: TrustClaimSource::CanonicalDecisionEvidence,
                boundary: TrustClaimBoundary::SupportedDelegatedFlowsOnly,
                note: Some("contract note".to_string()),
            },
        ],
    };

    let canonical = String::from_utf8(
        to_canonical_json_bytes(&trust_basis).expect("canonical trust basis json"),
    )
    .expect("canonical json is utf8");

    assert_eq!(
        canonical,
        r#"{
  "claims": [
    {
      "id": "bundle_verified",
      "level": "verified",
      "source": "bundle_verification",
      "boundary": "bundle-wide",
      "note": null
    },
    {
      "id": "delegation_context_visible",
      "level": "absent",
      "source": "canonical_decision_evidence",
      "boundary": "supported-delegated-flows-only",
      "note": "contract note"
    }
  ]
}
"#
    );
}

#[test]
fn trust_basis_regeneration_is_byte_stable() {
    let bundle = make_bundle(vec![make_event(
        "assay.tool.decision",
        "run_stable",
        0,
        json!({
            "tool": "tool.commit",
            "decision": "allow",
            "delegated_from": "agent:planner"
        }),
    )]);

    let first = generate_trust_basis(
        Cursor::new(&bundle),
        VerifyLimits::default(),
        TrustBasisOptions::default(),
    )
    .expect("first trust basis");
    let second = generate_trust_basis(
        Cursor::new(&bundle),
        VerifyLimits::default(),
        TrustBasisOptions::default(),
    )
    .expect("second trust basis");

    assert_eq!(
        to_canonical_json_bytes(&first).expect("first json"),
        to_canonical_json_bytes(&second).expect("second json")
    );
}

#[test]
fn trust_basis_detects_supported_delegation_and_degradation() {
    let bundle = make_bundle(vec![
        make_event(
            "assay.tool.decision",
            "run_signals",
            0,
            json!({
                "tool": "tool.commit",
                "decision": "allow",
                "delegated_from": "agent:planner"
            }),
        ),
        make_event(
            "assay.sandbox.degraded",
            "run_signals",
            1,
            json!({
                "reason_code": "policy_conflict",
                "degradation_mode": "audit_fallback",
                "component": "landlock"
            }),
        ),
    ]);

    let trust_basis = generate_trust_basis(
        Cursor::new(bundle),
        VerifyLimits::default(),
        TrustBasisOptions::default(),
    )
    .expect("trust basis should generate");

    assert_eq!(
        claim(&trust_basis, TrustClaimId::DelegationContextVisible).level,
        TrustClaimLevel::Verified
    );
    assert_eq!(
        claim(&trust_basis, TrustClaimId::ContainmentDegradationObserved).level,
        TrustClaimLevel::Verified
    );
    assert_eq!(
        claim(&trust_basis, TrustClaimId::AuthorizationContextVisible).level,
        TrustClaimLevel::Absent
    );
}
