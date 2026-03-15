//! Delegation capability spoofing with provenance ambiguity.
//!
//! Tests whether schema-valid, protocol-plausible capability claims,
//! provenance signals, or identity metadata from a delegated actor can cause
//! downstream consumers to silently upgrade trust or weaken classification.
//!
//! 4 attack vectors + 3 benign controls. All deterministic, no LLM calls.

use crate::report::{AttackResult, AttackStatus};
use assay_adapter_api::{
    AdapterCapabilities, AdapterDescriptor, LossinessLevel, LossinessReport, ProtocolDescriptor,
    RawPayloadRef,
};
use serde::Serialize;
use std::time::Instant;

#[derive(Debug, Clone, Serialize)]
pub struct DelegationResult {
    pub vector_id: String,
    pub condition: String,
    pub phase_a_injected: bool,
    pub trigger_activated: bool,
    pub claim_accepted: bool,
    pub expected_trust_level: String,
    pub observed_trust_level: String,
    pub outcome: DelegationOutcome,
    pub hypothesis_tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DelegationOutcome {
    NoEffect,
    RetainedNoActivation,
    ActivationWithCorrectDetection,
    ActivationWithTrustUpgrade,
    ActivationWithSelectionManipulation,
}

const TRUSTED_ADAPTER_ID: &str = "assay-adapter-acp";
const UNTRUSTED_ADAPTER_ID: &str = "assay-adapter-acp-fork";

fn clean_capabilities() -> AdapterCapabilities {
    AdapterCapabilities {
        supported_event_types: vec!["tool.decision".to_string()],
        supported_spec_versions: vec![">=1.0 <2.0".to_string()],
        supports_strict: true,
        supports_lenient: true,
    }
}

fn clean_protocol() -> ProtocolDescriptor {
    ProtocolDescriptor {
        name: "acp".to_string(),
        spec_version: "1.0".to_string(),
        schema_id: Some("acp.packet".to_string()),
        spec_url: None,
    }
}

fn clean_lossiness() -> LossinessReport {
    LossinessReport {
        lossiness_level: LossinessLevel::None,
        unmapped_fields_count: 0,
        raw_payload_ref: Some(RawPayloadRef {
            sha256: "sha256:abc123def456".to_string(),
            size_bytes: 1024,
            media_type: "application/json".to_string(),
        }),
        notes: vec![],
    }
}

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
            // Integrity only: check source URN consistency (both use same URN → not detected)
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

    // Same adapter_id, different version → legitimate migration
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

// ---------------------------------------------------------------------------
// Full matrix runner
// ---------------------------------------------------------------------------

pub fn run_delegation_spoofing_matrix() -> (Vec<DelegationResult>, Vec<AttackResult>) {
    let mut results = Vec::new();
    let mut attacks = Vec::new();

    for condition in ["condition_a", "condition_b", "condition_c"] {
        let (dr, ar) = vector1_capability_overclaim(condition);
        results.push(dr);
        attacks.push(ar);
        let (dr, ar) = vector2_provenance_ambiguity(condition);
        results.push(dr);
        attacks.push(ar);
        let (dr, ar) = vector3_identity_spoofing(condition);
        results.push(dr);
        attacks.push(ar);
        let (dr, ar) = vector4_selection_manipulation(condition);
        results.push(dr);
        attacks.push(ar);

        let (dr, ar) = control_d1_legitimate_upgrade(condition);
        results.push(dr);
        attacks.push(ar);
        let (dr, ar) = control_d2_legitimate_lossy(condition);
        results.push(dr);
        attacks.push(ar);
        let (dr, ar) = control_d3_adapter_migration(condition);
        results.push(dr);
        attacks.push(ar);
    }

    (results, attacks)
}

#[allow(clippy::too_many_arguments)]
fn make_delegation_result(
    vector: &str,
    condition: &str,
    claim_accepted: bool,
    expected: &str,
    observed: &str,
    outcome: DelegationOutcome,
    tags: Vec<String>,
    start: Instant,
) -> (DelegationResult, AttackResult) {
    let dr = DelegationResult {
        vector_id: vector.to_string(),
        condition: condition.to_string(),
        phase_a_injected: true,
        trigger_activated: true,
        claim_accepted,
        expected_trust_level: expected.to_string(),
        observed_trust_level: observed.to_string(),
        outcome: outcome.clone(),
        hypothesis_tags: tags,
    };
    let status = match &outcome {
        DelegationOutcome::ActivationWithTrustUpgrade
        | DelegationOutcome::ActivationWithSelectionManipulation => AttackStatus::Bypassed,
        DelegationOutcome::ActivationWithCorrectDetection => AttackStatus::Blocked,
        _ => AttackStatus::Passed,
    };
    let ar = AttackResult {
        name: format!("delegation.{}.{}", vector, condition),
        status,
        error_class: None,
        error_code: None,
        message: Some(format!(
            "expected={} observed={} outcome={:?}",
            expected, observed, outcome
        )),
        duration_ms: start.elapsed().as_millis() as u64,
    };
    (dr, ar)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v1_bypasses_under_condition_a() {
        let (dr, _) = vector1_capability_overclaim("condition_a");
        assert_eq!(dr.outcome, DelegationOutcome::ActivationWithTrustUpgrade);
    }

    #[test]
    fn v1_detected_under_condition_b() {
        let (dr, _) = vector1_capability_overclaim("condition_b");
        assert_eq!(
            dr.outcome,
            DelegationOutcome::ActivationWithCorrectDetection
        );
    }

    #[test]
    fn v2_bypasses_under_condition_a() {
        let (dr, _) = vector2_provenance_ambiguity("condition_a");
        assert_eq!(dr.outcome, DelegationOutcome::ActivationWithTrustUpgrade);
    }

    #[test]
    fn v2_detected_under_condition_b() {
        let (dr, _) = vector2_provenance_ambiguity("condition_b");
        assert_eq!(
            dr.outcome,
            DelegationOutcome::ActivationWithCorrectDetection
        );
    }

    #[test]
    fn v3_bypasses_under_condition_a_and_b() {
        let (dr_a, _) = vector3_identity_spoofing("condition_a");
        let (dr_b, _) = vector3_identity_spoofing("condition_b");
        assert_eq!(dr_a.outcome, DelegationOutcome::ActivationWithTrustUpgrade);
        assert_eq!(dr_b.outcome, DelegationOutcome::ActivationWithTrustUpgrade);
    }

    #[test]
    fn v3_detected_under_condition_c() {
        let (dr, _) = vector3_identity_spoofing("condition_c");
        assert_eq!(
            dr.outcome,
            DelegationOutcome::ActivationWithCorrectDetection
        );
    }

    #[test]
    fn v4_selects_malicious_under_condition_a() {
        let (dr, _) = vector4_selection_manipulation("condition_a");
        assert_eq!(
            dr.outcome,
            DelegationOutcome::ActivationWithSelectionManipulation
        );
    }

    #[test]
    fn v4_selects_legitimate_under_condition_b() {
        let (dr, _) = vector4_selection_manipulation("condition_b");
        assert_eq!(
            dr.outcome,
            DelegationOutcome::ActivationWithCorrectDetection
        );
    }

    #[test]
    fn controls_no_false_positives() {
        for cond in ["condition_a", "condition_b", "condition_c"] {
            let (d1, _) = control_d1_legitimate_upgrade(cond);
            let (d2, _) = control_d2_legitimate_lossy(cond);
            let (d3, _) = control_d3_adapter_migration(cond);
            assert_eq!(
                d1.outcome,
                DelegationOutcome::NoEffect,
                "D1 FP under {}",
                cond
            );
            assert_eq!(
                d2.outcome,
                DelegationOutcome::NoEffect,
                "D2 FP under {}",
                cond
            );
            assert_eq!(
                d3.outcome,
                DelegationOutcome::NoEffect,
                "D3 FP under {}",
                cond
            );
        }
    }

    #[test]
    fn full_matrix_structure() {
        let (results, attacks) = run_delegation_spoofing_matrix();
        assert_eq!(results.len(), 21); // 3 conditions * 7 (4 vectors + 3 controls)
        assert_eq!(attacks.len(), 21);
    }
}
