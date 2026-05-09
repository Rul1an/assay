use crate::bundle::VerifyLimits;
use anyhow::Result;
use serde::Serialize;
use std::io::Read;

mod classifiers;
mod diff;
mod generation;
mod types;

pub use diff::{diff_trust_basis, duplicate_trust_basis_claim_ids};
pub use types::{
    TrustBasis, TrustBasisClaim, TrustBasisClaimLevelDiff, TrustBasisClaimMetadataDiff,
    TrustBasisClaimPresenceDiff, TrustBasisDiffClass, TrustBasisDiffReport, TrustBasisDiffSummary,
    TrustBasisOptions, TrustClaimBoundary, TrustClaimId, TrustClaimLevel, TrustClaimSource,
    TRUST_BASIS_DIFF_SCHEMA,
};

pub fn generate_trust_basis<R: Read>(
    reader: R,
    limits: VerifyLimits,
    options: TrustBasisOptions,
) -> Result<TrustBasis> {
    generation::generate_trust_basis(reader, limits, options)
}

pub fn to_canonical_json_bytes(trust_basis: &TrustBasis) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"  ");
    let mut serializer = serde_json::Serializer::with_formatter(&mut output, formatter);
    trust_basis.serialize(&mut serializer)?;
    output.push(b'\n');
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundle::BundleWriter;
    use crate::lint::engine::LintOptions;
    use crate::lint::packs::load_pack;
    use crate::trust_basis::classifiers::SOURCE_ARTIFACT_REF_MAX_CHARS;
    use crate::types::EvidenceEvent;
    use chrono::{TimeZone, Utc};
    use serde_json::json;
    use std::io::Cursor;

    fn make_event(
        type_: &str,
        run_id: &str,
        seq: u64,
        payload: serde_json::Value,
    ) -> EvidenceEvent {
        let mut event =
            EvidenceEvent::new(type_, "urn:assay:test:trust-basis", run_id, seq, payload);
        event.time = Utc.timestamp_opt(1_700_000_000 + seq as i64, 0).unwrap();
        event
    }

    fn make_bundle(events: Vec<EvidenceEvent>) -> Vec<u8> {
        let mut buffer = Vec::new();
        let mut writer = BundleWriter::new(&mut buffer);
        for event in events {
            writer.add_event(event);
        }
        writer.finish().expect("bundle should finish");
        buffer
    }

    fn claim(trust_basis: &TrustBasis, id: TrustClaimId) -> &TrustBasisClaim {
        trust_basis
            .claims
            .iter()
            .find(|claim| claim.id == id)
            .expect("claim should exist")
    }

    fn trust_basis_claim(
        id: TrustClaimId,
        level: TrustClaimLevel,
        note: Option<&str>,
    ) -> TrustBasisClaim {
        TrustBasisClaim {
            id,
            level,
            source: TrustClaimSource::ExternalEvidenceReceipt,
            boundary: TrustClaimBoundary::SupportedExternalEvalReceiptEventsOnly,
            note: note.map(str::to_string),
        }
    }

    fn promptfoo_receipt_payload(extra: serde_json::Value) -> serde_json::Value {
        let mut payload = json!({
            "schema": "assay.receipt.promptfoo.assertion-component.v1",
            "source_system": "promptfoo",
            "source_surface": "cli-jsonl.gradingResult.componentResults",
            "source_artifact_ref": "results.jsonl",
            "source_artifact_digest": format!("sha256:{}", "a".repeat(64)),
            "reducer_version": "assay-promptfoo-jsonl-component-result@0.1.0",
            "imported_at": "2026-04-26T12:00:00Z",
            "assertion_type": "equals",
            "result": {
                "pass": true,
                "score": 1,
                "reason": "Assertion passed"
            }
        });
        if let Some(extra) = extra.as_object() {
            let obj = payload.as_object_mut().expect("payload object");
            for (key, value) in extra {
                obj.insert(key.clone(), value.clone());
            }
        }
        payload
    }

    fn openfeature_decision_receipt_payload(extra: serde_json::Value) -> serde_json::Value {
        let mut payload = json!({
            "schema": "assay.receipt.openfeature.evaluation_details.v1",
            "source_system": "openfeature",
            "source_surface": "evaluation_details.boolean",
            "source_artifact_ref": "openfeature-details.jsonl",
            "source_artifact_digest": format!("sha256:{}", "c".repeat(64)),
            "reducer_version": "assay-openfeature-evaluation-details@0.1.0",
            "imported_at": "2026-04-27T12:00:00Z",
            "decision": {
                "flag_key": "checkout.new_flow",
                "value_type": "boolean",
                "value": true,
                "variant": "on",
                "reason": "STATIC"
            }
        });
        if let Some(extra) = extra.as_object() {
            let obj = payload.as_object_mut().expect("payload object");
            for (key, value) in extra {
                obj.insert(key.clone(), value.clone());
            }
        }
        payload
    }

    fn cyclonedx_mlbom_model_receipt_payload(extra: serde_json::Value) -> serde_json::Value {
        let mut payload = json!({
            "schema": "assay.receipt.cyclonedx.mlbom-model-component.v1",
            "source_system": "cyclonedx",
            "source_surface": "bom.components[type=machine-learning-model]",
            "source_artifact_ref": "bom.cdx.json",
            "source_artifact_digest": format!("sha256:{}", "b".repeat(64)),
            "reducer_version": "assay-cyclonedx-mlbom-model-component@0.1.0",
            "imported_at": "2026-04-28T12:00:00Z",
            "model_component": {
                "bom_ref": "pkg:huggingface/example/model@abc123",
                "name": "example-model",
                "version": "1.0.0",
                "publisher": "Example Inc.",
                "purl": "pkg:huggingface/example/model@abc123",
                "dataset_refs": ["component-training-data"],
                "model_card_refs": ["model-card-example-model"]
            }
        });
        if let Some(extra) = extra.as_object() {
            let obj = payload.as_object_mut().expect("payload object");
            for (key, value) in extra {
                obj.insert(key.clone(), value.clone());
            }
        }
        payload
    }

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
}
