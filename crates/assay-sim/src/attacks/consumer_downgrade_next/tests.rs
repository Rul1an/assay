use super::*;

#[test]
fn v1_downgrades_under_a() {
    let (cr, _) = vector1_partial_trust_read("condition_a");
    assert_eq!(cr.outcome, ConsumerOutcome::SilentDowngrade);
}

#[test]
fn v1_blocked_under_b() {
    let (cr, _) = vector1_partial_trust_read("condition_b");
    assert_eq!(cr.outcome, ConsumerOutcome::NoEffect);
}

#[test]
fn v2_downgrades_under_a_and_b() {
    let (cr_a, _) = vector2_precedence_inversion("condition_a");
    let (cr_b, _) = vector2_precedence_inversion("condition_b");
    assert_eq!(cr_a.outcome, ConsumerOutcome::SilentDowngrade);
    assert_eq!(cr_b.outcome, ConsumerOutcome::SilentDowngrade);
}

#[test]
fn v2_blocked_under_c() {
    let (cr, _) = vector2_precedence_inversion("condition_c");
    assert_eq!(cr.outcome, ConsumerOutcome::NoEffect);
}

#[test]
fn v3_flattens_under_a() {
    let (cr_a, _) = vector3_compat_flattening("condition_a");
    assert_ne!(
        cr_a.outcome,
        ConsumerOutcome::NoEffect,
        "V3 under A should show some effect, got {:?}",
        cr_a.outcome
    );
}

#[test]
fn v3_flattens_or_detected_under_b() {
    let (cr_b, _) = vector3_compat_flattening("condition_b");
    // B treats compat as non-binding: flattening may produce different bucket
    // or may collapse to same if classify_replay_diff fields happen to match.
    assert_ne!(
        cr_b.outcome,
        ConsumerOutcome::SilentTrustUpgrade,
        "V3 under B should not produce trust upgrade"
    );
}

#[test]
fn v3_clean_under_c() {
    let (cr, _) = vector3_compat_flattening("condition_c");
    assert_eq!(cr.outcome, ConsumerOutcome::NoEffect);
}

#[test]
fn v4_downgrades_under_a_and_b() {
    let (cr_a, _) = vector4_projection_loss("condition_a");
    let (cr_b, _) = vector4_projection_loss("condition_b");
    assert_eq!(cr_a.outcome, ConsumerOutcome::SilentDowngrade);
    assert_eq!(cr_b.outcome, ConsumerOutcome::SilentDowngrade);
}

#[test]
fn v4_detected_under_c() {
    let (cr, _) = vector4_projection_loss("condition_c");
    assert_eq!(cr.outcome, ConsumerOutcome::DowngradeWithCorrectDetection);
}

#[test]
fn controls_no_false_positives() {
    for cond in ["condition_a", "condition_b", "condition_c"] {
        let (e1, _) = control_e1_legitimate_legacy(cond);
        let (e2, _) = control_e2_legitimate_compat(cond);
        let (e3, _) = control_e3_legitimate_converged(cond);
        assert_eq!(
            e1.outcome,
            ConsumerOutcome::NoEffect,
            "E1 FP under {}",
            cond
        );
        assert_eq!(
            e2.outcome,
            ConsumerOutcome::NoEffect,
            "E2 FP under {}",
            cond
        );
        assert_eq!(
            e3.outcome,
            ConsumerOutcome::NoEffect,
            "E3 FP under {}",
            cond
        );
    }
}

#[test]
fn full_matrix_structure() {
    let (results, attacks) = run_consumer_downgrade_matrix();
    assert_eq!(results.len(), 21); // 3 conditions * 7 (4 vectors + 3 controls)
    assert_eq!(attacks.len(), 21);
}
