use super::controls::{
    control_d1_legitimate_upgrade, control_d2_legitimate_lossy, control_d3_adapter_migration,
};
use super::vectors::{
    vector1_capability_overclaim, vector2_provenance_ambiguity, vector3_identity_spoofing,
    vector4_selection_manipulation,
};
use super::{DelegationOutcome, DelegationResult};
use crate::report::{AttackResult, AttackStatus};
use std::time::Instant;

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
pub(super) fn make_delegation_result(
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
