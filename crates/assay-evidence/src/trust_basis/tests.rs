use super::*;
use crate::bundle::BundleWriter;
use crate::lint::engine::LintOptions;
use crate::lint::packs::load_pack;
use crate::trust_basis::classifiers::SOURCE_ARTIFACT_REF_MAX_CHARS;
use crate::types::EvidenceEvent;
use chrono::{TimeZone, Utc};
use serde_json::json;
use std::io::Cursor;

fn make_event(type_: &str, run_id: &str, seq: u64, payload: serde_json::Value) -> EvidenceEvent {
    let mut event = EvidenceEvent::new(type_, "urn:assay:test:trust-basis", run_id, seq, payload);
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

mod claim_authorization_pack;
mod claim_contract;
mod claim_diff_limits;
mod claim_receipts;
