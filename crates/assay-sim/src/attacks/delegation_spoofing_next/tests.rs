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
