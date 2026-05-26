use super::*;
#[test]
fn trust_basis_detects_supported_external_eval_receipt_boundary() {
    let bundle = make_bundle(vec![make_event(
        "assay.receipt.promptfoo.assertion_component.v1",
        "run_promptfoo_receipt",
        0,
        promptfoo_receipt_payload(json!({})),
    )]);

    let trust_basis = generate_trust_basis(
        Cursor::new(bundle),
        VerifyLimits::default(),
        TrustBasisOptions::default(),
    )
    .expect("trust basis should generate");

    assert_eq!(
        claim(
            &trust_basis,
            TrustClaimId::ExternalEvalReceiptBoundaryVisible
        )
        .level,
        TrustClaimLevel::Verified
    );
}

#[test]
fn trust_basis_rejects_promptfoo_receipt_boundary_when_raw_payload_leaks_in() {
    let bundle = make_bundle(vec![make_event(
        "assay.receipt.promptfoo.assertion_component.v1",
        "run_promptfoo_raw",
        0,
        promptfoo_receipt_payload(json!({
            "output": "raw model output should not be in the receipt"
        })),
    )]);

    let trust_basis = generate_trust_basis(
        Cursor::new(bundle),
        VerifyLimits::default(),
        TrustBasisOptions::default(),
    )
    .expect("trust basis should generate");

    assert_eq!(
        claim(
            &trust_basis,
            TrustClaimId::ExternalEvalReceiptBoundaryVisible
        )
        .level,
        TrustClaimLevel::Absent
    );
}

#[test]
fn trust_basis_rejects_promptfoo_receipt_boundary_when_import_time_is_not_utc_rfc3339() {
    let bundle = make_bundle(vec![make_event(
        "assay.receipt.promptfoo.assertion_component.v1",
        "run_promptfoo_bad_time",
        0,
        promptfoo_receipt_payload(json!({
            "imported_at": "not-a-timestamp"
        })),
    )]);

    let trust_basis = generate_trust_basis(
        Cursor::new(bundle),
        VerifyLimits::default(),
        TrustBasisOptions::default(),
    )
    .expect("trust basis should generate");

    assert_eq!(
        claim(
            &trust_basis,
            TrustClaimId::ExternalEvalReceiptBoundaryVisible
        )
        .level,
        TrustClaimLevel::Absent
    );
}

#[test]
fn trust_basis_detects_supported_external_decision_receipt_boundary() {
    let bundle = make_bundle(vec![make_event(
        "assay.receipt.openfeature.evaluation_details.v1",
        "run_openfeature_receipt",
        0,
        openfeature_decision_receipt_payload(json!({})),
    )]);

    let trust_basis = generate_trust_basis(
        Cursor::new(bundle),
        VerifyLimits::default(),
        TrustBasisOptions::default(),
    )
    .expect("trust basis should generate");

    assert_eq!(
        claim(
            &trust_basis,
            TrustClaimId::ExternalDecisionReceiptBoundaryVisible
        )
        .level,
        TrustClaimLevel::Verified
    );
    assert_eq!(
        claim(
            &trust_basis,
            TrustClaimId::ExternalEvalReceiptBoundaryVisible
        )
        .level,
        TrustClaimLevel::Absent
    );
    assert_eq!(
        claim(
            &trust_basis,
            TrustClaimId::ExternalInventoryReceiptBoundaryVisible
        )
        .level,
        TrustClaimLevel::Absent
    );
}

#[test]
fn trust_basis_rejects_decision_receipt_boundary_when_context_or_metadata_leaks_in() {
    for (case_name, extra) in [
        (
            "run_openfeature_context",
            json!({ "evaluation_context": { "targeting_key": "user-123" } }),
        ),
        (
            "run_openfeature_metadata",
            json!({
                "decision": {
                    "flag_key": "checkout.new_flow",
                    "value_type": "boolean",
                    "value": true,
                    "flag_metadata": { "provider": "out-of-scope" }
                }
            }),
        ),
        (
            "run_openfeature_error_message",
            json!({
                "decision": {
                    "flag_key": "checkout.new_flow",
                    "value_type": "boolean",
                    "value": false,
                    "reason": "ERROR",
                    "error_code": "FLAG_NOT_FOUND",
                    "error_message": "provider message is out of scope"
                }
            }),
        ),
    ] {
        let bundle = make_bundle(vec![make_event(
            "assay.receipt.openfeature.evaluation_details.v1",
            case_name,
            0,
            openfeature_decision_receipt_payload(extra),
        )]);

        let trust_basis = generate_trust_basis(
            Cursor::new(bundle),
            VerifyLimits::default(),
            TrustBasisOptions::default(),
        )
        .expect("trust basis should generate");

        assert_eq!(
            claim(
                &trust_basis,
                TrustClaimId::ExternalDecisionReceiptBoundaryVisible
            )
            .level,
            TrustClaimLevel::Absent
        );
    }
}

#[test]
fn trust_basis_rejects_decision_receipt_boundary_when_value_type_or_value_is_not_boolean() {
    for (case_name, decision) in [
        (
            "run_openfeature_wrong_value_type",
            json!({
                "flag_key": "checkout.new_flow",
                "value_type": "string",
                "value": true
            }),
        ),
        (
            "run_openfeature_string_value",
            json!({
                "flag_key": "checkout.new_flow",
                "value_type": "boolean",
                "value": "on"
            }),
        ),
    ] {
        let bundle = make_bundle(vec![make_event(
            "assay.receipt.openfeature.evaluation_details.v1",
            case_name,
            0,
            openfeature_decision_receipt_payload(json!({ "decision": decision })),
        )]);

        let trust_basis = generate_trust_basis(
            Cursor::new(bundle),
            VerifyLimits::default(),
            TrustBasisOptions::default(),
        )
        .expect("trust basis should generate");

        assert_eq!(
            claim(
                &trust_basis,
                TrustClaimId::ExternalDecisionReceiptBoundaryVisible
            )
            .level,
            TrustClaimLevel::Absent
        );
    }
}

#[test]
fn trust_basis_detects_supported_external_inventory_receipt_boundary() {
    let bundle = make_bundle(vec![make_event(
        "assay.receipt.cyclonedx.mlbom_model_component.v1",
        "run_cyclonedx_receipt",
        0,
        cyclonedx_mlbom_model_receipt_payload(json!({})),
    )]);

    let trust_basis = generate_trust_basis(
        Cursor::new(bundle),
        VerifyLimits::default(),
        TrustBasisOptions::default(),
    )
    .expect("trust basis should generate");

    assert_eq!(
        claim(
            &trust_basis,
            TrustClaimId::ExternalInventoryReceiptBoundaryVisible
        )
        .level,
        TrustClaimLevel::Verified
    );
    assert_eq!(
        claim(
            &trust_basis,
            TrustClaimId::ExternalEvalReceiptBoundaryVisible
        )
        .level,
        TrustClaimLevel::Absent
    );
}

#[test]
fn trust_basis_rejects_inventory_receipt_boundary_when_model_card_body_leaks_in() {
    let bundle = make_bundle(vec![make_event(
        "assay.receipt.cyclonedx.mlbom_model_component.v1",
        "run_cyclonedx_raw_model_card",
        0,
        cyclonedx_mlbom_model_receipt_payload(json!({
            "model_component": {
                "bom_ref": "pkg:huggingface/example/model@abc123",
                "name": "example-model",
                "modelCard": {
                    "modelParameters": {
                        "datasets": [{ "ref": "component-training-data" }]
                    }
                }
            }
        })),
    )]);

    let trust_basis = generate_trust_basis(
        Cursor::new(bundle),
        VerifyLimits::default(),
        TrustBasisOptions::default(),
    )
    .expect("trust basis should generate");

    assert_eq!(
        claim(
            &trust_basis,
            TrustClaimId::ExternalInventoryReceiptBoundaryVisible
        )
        .level,
        TrustClaimLevel::Absent
    );
}

#[test]
fn trust_basis_rejects_inventory_receipt_boundary_when_digest_is_missing_or_bad() {
    let bundle = make_bundle(vec![make_event(
        "assay.receipt.cyclonedx.mlbom_model_component.v1",
        "run_cyclonedx_bad_digest",
        0,
        cyclonedx_mlbom_model_receipt_payload(json!({
            "source_artifact_digest": "sha256:not-a-real-digest"
        })),
    )]);

    let trust_basis = generate_trust_basis(
        Cursor::new(bundle),
        VerifyLimits::default(),
        TrustBasisOptions::default(),
    )
    .expect("trust basis should generate");

    assert_eq!(
        claim(
            &trust_basis,
            TrustClaimId::ExternalInventoryReceiptBoundaryVisible
        )
        .level,
        TrustClaimLevel::Absent
    );
}

#[test]
fn trust_basis_rejects_inventory_receipt_boundary_when_source_ref_is_unbounded() {
    for (case_name, source_artifact_ref) in [
        ("control_char", "bom.cdx.json\nnext-line".to_string()),
        ("too_long", "x".repeat(SOURCE_ARTIFACT_REF_MAX_CHARS + 1)),
    ] {
        let bundle = make_bundle(vec![make_event(
            "assay.receipt.cyclonedx.mlbom_model_component.v1",
            case_name,
            0,
            cyclonedx_mlbom_model_receipt_payload(json!({
                "source_artifact_ref": source_artifact_ref
            })),
        )]);

        let trust_basis = generate_trust_basis(
            Cursor::new(bundle),
            VerifyLimits::default(),
            TrustBasisOptions::default(),
        )
        .expect("trust basis should generate");

        assert_eq!(
            claim(
                &trust_basis,
                TrustClaimId::ExternalInventoryReceiptBoundaryVisible
            )
            .level,
            TrustClaimLevel::Absent
        );
    }
}

#[test]
fn trust_basis_rejects_inventory_receipt_boundary_when_refs_are_expanded_objects() {
    let bundle = make_bundle(vec![make_event(
        "assay.receipt.cyclonedx.mlbom_model_component.v1",
        "run_cyclonedx_expanded_dataset",
        0,
        cyclonedx_mlbom_model_receipt_payload(json!({
            "model_component": {
                "bom_ref": "pkg:huggingface/example/model@abc123",
                "name": "example-model",
                "dataset_refs": [{ "ref": "component-training-data", "name": "raw dataset body" }]
            }
        })),
    )]);

    let trust_basis = generate_trust_basis(
        Cursor::new(bundle),
        VerifyLimits::default(),
        TrustBasisOptions::default(),
    )
    .expect("trust basis should generate");

    assert_eq!(
        claim(
            &trust_basis,
            TrustClaimId::ExternalInventoryReceiptBoundaryVisible
        )
        .level,
        TrustClaimLevel::Absent
    );
}
