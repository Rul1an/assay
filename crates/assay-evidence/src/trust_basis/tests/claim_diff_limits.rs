use super::*;
#[test]
fn trust_basis_diff_keys_by_claim_id_and_reports_note_metadata_nonblocking() {
    let baseline = TrustBasis {
        claims: vec![trust_basis_claim(
            TrustClaimId::ExternalEvalReceiptBoundaryVisible,
            TrustClaimLevel::Verified,
            Some("old note"),
        )],
    };
    let candidate = TrustBasis {
        claims: vec![trust_basis_claim(
            TrustClaimId::ExternalEvalReceiptBoundaryVisible,
            TrustClaimLevel::Verified,
            Some("new note"),
        )],
    };

    let report = diff_trust_basis(&baseline, &candidate);

    assert_eq!(report.schema, TRUST_BASIS_DIFF_SCHEMA);
    assert_eq!(report.claim_identity, "claim.id");
    assert!(!report.has_regressions());
    assert!(report.regressed_claims.is_empty());
    assert!(report.removed_claims.is_empty());
    assert_eq!(report.metadata_changes.len(), 1);
    assert_eq!(
        report.metadata_changes[0].claim_id,
        TrustClaimId::ExternalEvalReceiptBoundaryVisible
    );
    assert!(report.metadata_changes[0].note_changed);
}

#[test]
fn trust_basis_diff_sorts_by_serialized_claim_id() {
    let baseline = TrustBasis { claims: vec![] };
    let candidate = TrustBasis {
        claims: vec![
            trust_basis_claim(
                TrustClaimId::BundleVerified,
                TrustClaimLevel::Verified,
                None,
            ),
            trust_basis_claim(
                TrustClaimId::AppliedPackFindingsPresent,
                TrustClaimLevel::Verified,
                None,
            ),
        ],
    };

    let report = diff_trust_basis(&baseline, &candidate);
    let added_ids: Vec<_> = report
        .added_claims
        .iter()
        .map(|diff| diff.claim_id)
        .collect();

    assert_eq!(
        added_ids,
        vec![
            TrustClaimId::AppliedPackFindingsPresent,
            TrustClaimId::BundleVerified,
        ]
    );
}

#[test]
fn trust_basis_contract_diff_report_ordering_is_frozen() {
    let baseline = TrustBasis {
        claims: vec![
            trust_basis_claim(
                TrustClaimId::SigningEvidencePresent,
                TrustClaimLevel::Verified,
                None,
            ),
            trust_basis_claim(
                TrustClaimId::ExternalEvalReceiptBoundaryVisible,
                TrustClaimLevel::Absent,
                None,
            ),
            trust_basis_claim(
                TrustClaimId::BundleVerified,
                TrustClaimLevel::Verified,
                Some("baseline note"),
            ),
            trust_basis_claim(
                TrustClaimId::AuthorizationContextVisible,
                TrustClaimLevel::Verified,
                None,
            ),
        ],
    };
    let candidate = TrustBasis {
        claims: vec![
            trust_basis_claim(
                TrustClaimId::ExternalEvalReceiptBoundaryVisible,
                TrustClaimLevel::Verified,
                None,
            ),
            trust_basis_claim(
                TrustClaimId::BundleVerified,
                TrustClaimLevel::Verified,
                Some("candidate note"),
            ),
            trust_basis_claim(
                TrustClaimId::AppliedPackFindingsPresent,
                TrustClaimLevel::Verified,
                None,
            ),
            trust_basis_claim(
                TrustClaimId::SigningEvidencePresent,
                TrustClaimLevel::Absent,
                None,
            ),
        ],
    };

    let report = diff_trust_basis(&baseline, &candidate);

    assert_eq!(report.schema, TRUST_BASIS_DIFF_SCHEMA);
    assert_eq!(report.claim_identity, "claim.id");
    assert_eq!(
        report.level_order,
        vec![
            TrustClaimLevel::Absent,
            TrustClaimLevel::Inferred,
            TrustClaimLevel::SelfReported,
            TrustClaimLevel::Verified,
        ]
    );
    assert_eq!(report.summary.regressed_claims, 1);
    assert_eq!(report.summary.improved_claims, 1);
    assert_eq!(report.summary.removed_claims, 1);
    assert_eq!(report.summary.added_claims, 1);
    assert_eq!(report.summary.metadata_changes, 1);
    assert_eq!(report.summary.unchanged_claim_count, 0);
    assert!(report.summary.has_regressions);
    assert_eq!(
        report
            .regressed_claims
            .iter()
            .map(|diff| diff.claim_id)
            .collect::<Vec<_>>(),
        vec![TrustClaimId::SigningEvidencePresent]
    );
    assert_eq!(
        report
            .improved_claims
            .iter()
            .map(|diff| diff.claim_id)
            .collect::<Vec<_>>(),
        vec![TrustClaimId::ExternalEvalReceiptBoundaryVisible]
    );
    assert_eq!(
        report
            .removed_claims
            .iter()
            .map(|diff| diff.claim_id)
            .collect::<Vec<_>>(),
        vec![TrustClaimId::AuthorizationContextVisible]
    );
    assert_eq!(
        report
            .added_claims
            .iter()
            .map(|diff| diff.claim_id)
            .collect::<Vec<_>>(),
        vec![TrustClaimId::AppliedPackFindingsPresent]
    );
    assert_eq!(
        report
            .metadata_changes
            .iter()
            .map(|diff| diff.claim_id)
            .collect::<Vec<_>>(),
        vec![TrustClaimId::BundleVerified]
    );
}

#[test]
fn trust_basis_diff_dedupes_duplicate_claim_ids_for_library_consumers() {
    let baseline = TrustBasis {
        claims: vec![
            trust_basis_claim(
                TrustClaimId::BundleVerified,
                TrustClaimLevel::Verified,
                None,
            ),
            trust_basis_claim(TrustClaimId::BundleVerified, TrustClaimLevel::Absent, None),
        ],
    };
    let candidate = TrustBasis {
        claims: vec![
            trust_basis_claim(
                TrustClaimId::ExternalEvalReceiptBoundaryVisible,
                TrustClaimLevel::Verified,
                None,
            ),
            trust_basis_claim(
                TrustClaimId::ExternalEvalReceiptBoundaryVisible,
                TrustClaimLevel::Absent,
                None,
            ),
        ],
    };

    let report = diff_trust_basis(&baseline, &candidate);

    assert_eq!(report.removed_claims.len(), 1);
    assert_eq!(
        report.removed_claims[0].claim_id,
        TrustClaimId::BundleVerified
    );
    assert_eq!(report.added_claims.len(), 1);
    assert_eq!(
        report.added_claims[0].claim_id,
        TrustClaimId::ExternalEvalReceiptBoundaryVisible
    );
}

#[test]
fn duplicate_trust_basis_claim_ids_are_reported_deterministically() {
    let trust_basis = TrustBasis {
        claims: vec![
            trust_basis_claim(
                TrustClaimId::BundleVerified,
                TrustClaimLevel::Verified,
                None,
            ),
            trust_basis_claim(
                TrustClaimId::ExternalEvalReceiptBoundaryVisible,
                TrustClaimLevel::Verified,
                None,
            ),
            trust_basis_claim(
                TrustClaimId::BundleVerified,
                TrustClaimLevel::Verified,
                None,
            ),
            trust_basis_claim(
                TrustClaimId::ExternalEvalReceiptBoundaryVisible,
                TrustClaimLevel::Verified,
                None,
            ),
        ],
    };

    assert_eq!(
        duplicate_trust_basis_claim_ids(&trust_basis),
        vec![
            TrustClaimId::BundleVerified,
            TrustClaimId::ExternalEvalReceiptBoundaryVisible,
        ]
    );
}

#[test]
fn trust_basis_respects_max_bundle_bytes_before_verification() {
    let bundle = make_bundle(vec![make_event(
        "assay.tool.decision",
        "run_size_limit",
        0,
        json!({
            "tool": "tool.commit",
            "decision": "allow"
        }),
    )]);

    let err = generate_trust_basis(
        Cursor::new(bundle),
        VerifyLimits {
            max_bundle_bytes: 8,
            ..VerifyLimits::default()
        },
        TrustBasisOptions::default(),
    )
    .expect_err("trust basis generation should fail when compressed input exceeds limit");

    assert!(
        err.to_string()
            .contains("trust basis bundle exceeds compressed input limit"),
        "unexpected error: {err}"
    );
}
