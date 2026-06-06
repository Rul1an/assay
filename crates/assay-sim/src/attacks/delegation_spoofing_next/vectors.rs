use super::fixtures::{
    clean_capabilities, clean_lossiness, clean_protocol, TRUSTED_ADAPTER_ID, UNTRUSTED_ADAPTER_ID,
};
use super::matrix::make_delegation_result;
use super::{DelegationOutcome, DelegationResult};
use crate::report::AttackResult;
use assay_adapter_api::{AdapterCapabilities, AdapterDescriptor, LossinessLevel, LossinessReport};
use std::time::Instant;

// ---------------------------------------------------------------------------
// Vector 1: Capability Overclaim
// ---------------------------------------------------------------------------

pub fn vector1_capability_overclaim(condition: &str) -> (DelegationResult, AttackResult) {
    let start = Instant::now();

    let clean = clean_capabilities();
    let overclaimed = AdapterCapabilities {
        supported_event_types: vec![
            "tool.decision".to_string(),
            "mandate.used".to_string(),
            "approval.granted".to_string(),
        ],
        ..clean.clone()
    };

    let observed_emissions = ["tool.decision"];

    let overclaim_detected = match condition {
        "condition_b" | "condition_c" => overclaimed
            .supported_event_types
            .iter()
            .any(|t| !observed_emissions.contains(&t.as_str())),
        _ => false,
    };

    let claim_accepted = !overclaim_detected;
    let outcome = if overclaim_detected {
        DelegationOutcome::ActivationWithCorrectDetection
    } else {
        DelegationOutcome::ActivationWithTrustUpgrade
    };

    make_delegation_result(
        "v1_capability_overclaim",
        condition,
        claim_accepted,
        "verified_only",
        if claim_accepted {
            "overclaimed_accepted"
        } else {
            "overclaim_detected"
        },
        outcome,
        vec!["H1".into()],
        start,
    )
}

// ---------------------------------------------------------------------------
// Vector 2: Provenance Ambiguity
// ---------------------------------------------------------------------------

pub fn vector2_provenance_ambiguity(condition: &str) -> (DelegationResult, AttackResult) {
    let start = Instant::now();

    let _clean = clean_lossiness();
    let poisoned = LossinessReport {
        lossiness_level: LossinessLevel::High,
        unmapped_fields_count: 3,
        raw_payload_ref: None,
        notes: vec!["source_id unmapped".to_string()],
    };

    let provenance_downgraded = match condition {
        "condition_b" | "condition_c" => {
            poisoned.raw_payload_ref.is_none() || poisoned.lossiness_level != LossinessLevel::None
        }
        _ => false,
    };

    let trust_preserved = match condition {
        "condition_c" => provenance_downgraded,
        "condition_b" => provenance_downgraded,
        _ => false,
    };

    let claim_accepted = !trust_preserved;
    let outcome = if trust_preserved {
        DelegationOutcome::ActivationWithCorrectDetection
    } else {
        DelegationOutcome::ActivationWithTrustUpgrade
    };

    make_delegation_result(
        "v2_provenance_ambiguity",
        condition,
        claim_accepted,
        "provenance_verified",
        if claim_accepted {
            "provenance_absent_accepted"
        } else {
            "provenance_downgraded"
        },
        outcome,
        vec!["H2".into()],
        start,
    )
}

// ---------------------------------------------------------------------------
// Vector 3: Delegation Identity Spoofing
// ---------------------------------------------------------------------------

pub fn vector3_identity_spoofing(condition: &str) -> (DelegationResult, AttackResult) {
    let start = Instant::now();

    let trusted = AdapterDescriptor {
        adapter_id: TRUSTED_ADAPTER_ID,
        adapter_version: "3.1.0",
    };
    let spoofed = AdapterDescriptor {
        adapter_id: UNTRUSTED_ADAPTER_ID,
        adapter_version: "3.1.0",
    };

    let source_urn = "urn:assay:adapter:acp";
    let _protocol = clean_protocol();

    let identity_verified = match condition {
        "condition_c" => {
            // Full trust stack: verify adapter_id matches trusted set, not just source URN
            spoofed.adapter_id == trusted.adapter_id
        }
        "condition_b" => {
            // Integrity only: check source URN consistency (both use same URN -> not detected)
            let _spoofed_source = source_urn;
            true // source URN matches, so B does not detect
        }
        _ => true, // unprotected
    };

    let spoof_succeeded = identity_verified && spoofed.adapter_id != trusted.adapter_id;
    let outcome = if !identity_verified {
        DelegationOutcome::ActivationWithCorrectDetection
    } else if spoof_succeeded {
        DelegationOutcome::ActivationWithTrustUpgrade
    } else {
        DelegationOutcome::NoEffect
    };

    make_delegation_result(
        "v3_identity_spoofing",
        condition,
        identity_verified,
        "trusted_domain",
        if spoof_succeeded {
            "spoofed_accepted"
        } else {
            "spoof_detected"
        },
        outcome,
        vec!["H4".into()],
        start,
    )
}

// ---------------------------------------------------------------------------
// Vector 4: Preference/Selection Manipulation
// ---------------------------------------------------------------------------

pub fn vector4_selection_manipulation(condition: &str) -> (DelegationResult, AttackResult) {
    let start = Instant::now();

    let adapter_a = AdapterCapabilities {
        supported_event_types: vec!["tool.decision".to_string()],
        ..Default::default()
    };
    let adapter_b = AdapterCapabilities {
        supported_event_types: vec!["tool.decision".to_string(), "mandate.used".to_string()],
        ..Default::default()
    };
    let malicious_c = AdapterCapabilities {
        supported_event_types: vec![
            "tool.decision".to_string(),
            "mandate.used".to_string(),
            "compliance.check".to_string(),
        ],
        ..Default::default()
    };

    let observed_a: &[&str] = &["tool.decision"];
    let observed_b: &[&str] = &["tool.decision", "mandate.used"];
    let observed_c: &[&str] = &["tool.decision"]; // malicious only produces tool.decision

    let select_by_breadth = |caps: &AdapterCapabilities| caps.supported_event_types.len();

    let selection = match condition {
        "condition_b" | "condition_c" => {
            let verified_a = observed_a.len();
            let verified_b = observed_b.len();
            let verified_c: usize = malicious_c
                .supported_event_types
                .iter()
                .filter(|t| observed_c.contains(&t.as_str()))
                .count();

            if verified_b >= verified_a && verified_b >= verified_c {
                "adapter_b"
            } else if verified_a >= verified_c {
                "adapter_a"
            } else {
                "adapter_c"
            }
        }
        _ => {
            // Unprotected: select by raw claim breadth
            let scores = [
                ("adapter_a", select_by_breadth(&adapter_a)),
                ("adapter_b", select_by_breadth(&adapter_b)),
                ("adapter_c", select_by_breadth(&malicious_c)),
            ];
            scores.iter().max_by_key(|s| s.1).unwrap().0
        }
    };

    let malicious_selected = selection == "adapter_c";
    let outcome = if malicious_selected {
        DelegationOutcome::ActivationWithSelectionManipulation
    } else {
        DelegationOutcome::ActivationWithCorrectDetection
    };

    make_delegation_result(
        "v4_selection_manipulation",
        condition,
        malicious_selected,
        "adapter_b",
        selection,
        outcome,
        vec!["H1".into()],
        start,
    )
}
