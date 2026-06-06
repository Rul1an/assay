use super::matrix::make_delegation_result;
use super::{DelegationOutcome, DelegationResult};
use crate::report::AttackResult;
use assay_adapter_api::{
    AdapterCapabilities, AdapterDescriptor, LossinessLevel, LossinessReport, RawPayloadRef,
};
use std::time::Instant;

// ---------------------------------------------------------------------------
// Benign Controls
// ---------------------------------------------------------------------------

/// D1: Legitimate adapter capability upgrade.
pub fn control_d1_legitimate_upgrade(condition: &str) -> (DelegationResult, AttackResult) {
    let start = Instant::now();

    let v2_caps = AdapterCapabilities {
        supported_event_types: vec!["tool.decision".to_string(), "mandate.used".to_string()],
        ..Default::default()
    };
    let v2_emissions: &[&str] = &["tool.decision", "mandate.used"];

    let flagged = match condition {
        "condition_b" | "condition_c" => v2_caps
            .supported_event_types
            .iter()
            .any(|t| !v2_emissions.contains(&t.as_str())),
        _ => false,
    };

    make_delegation_result(
        "control_d1_upgrade",
        condition,
        !flagged,
        "legitimate",
        if flagged {
            "false_positive"
        } else {
            "legitimate"
        },
        if flagged {
            DelegationOutcome::ActivationWithCorrectDetection
        } else {
            DelegationOutcome::NoEffect
        },
        vec!["H3".into()],
        start,
    )
}

/// D2: Legitimate high-lossiness conversion.
pub fn control_d2_legitimate_lossy(condition: &str) -> (DelegationResult, AttackResult) {
    let start = Instant::now();

    let lossy = LossinessReport {
        lossiness_level: LossinessLevel::Low,
        unmapped_fields_count: 1,
        raw_payload_ref: Some(RawPayloadRef {
            sha256: "sha256:legit_hash".to_string(),
            size_bytes: 512,
            media_type: "application/json".to_string(),
        }),
        notes: vec!["optional_field unmapped".to_string()],
    };

    // Legitimate low-lossiness with provenance should NOT be flagged
    let flagged = match condition {
        "condition_b" | "condition_c" => {
            lossy.raw_payload_ref.is_none() || lossy.lossiness_level == LossinessLevel::High
        }
        _ => false,
    };

    make_delegation_result(
        "control_d2_lossy",
        condition,
        !flagged,
        "legitimate",
        if flagged {
            "false_positive"
        } else {
            "legitimate"
        },
        if flagged {
            DelegationOutcome::ActivationWithCorrectDetection
        } else {
            DelegationOutcome::NoEffect
        },
        vec!["H3".into()],
        start,
    )
}

/// D3: Legitimate adapter migration (source change).
pub fn control_d3_adapter_migration(condition: &str) -> (DelegationResult, AttackResult) {
    let start = Instant::now();

    let old_adapter = AdapterDescriptor {
        adapter_id: "assay-adapter-acp",
        adapter_version: "3.0.0",
    };
    let new_adapter = AdapterDescriptor {
        adapter_id: "assay-adapter-acp",
        adapter_version: "3.1.0",
    };

    // Same adapter_id, different version -> legitimate migration
    let flagged = match condition {
        "condition_c" => old_adapter.adapter_id != new_adapter.adapter_id,
        _ => false,
    };

    make_delegation_result(
        "control_d3_migration",
        condition,
        !flagged,
        "legitimate",
        if flagged {
            "false_positive"
        } else {
            "legitimate"
        },
        if flagged {
            DelegationOutcome::ActivationWithCorrectDetection
        } else {
            DelegationOutcome::NoEffect
        },
        vec!["H3".into()],
        start,
    )
}
