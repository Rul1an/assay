//! Delegation spoofing invariant tests.

use assay_sim::attacks::delegation_spoofing::{run_delegation_spoofing_matrix, DelegationOutcome};

#[test]
fn overarching_invariant_controls_never_flag() {
    let (results, _) = run_delegation_spoofing_matrix();

    for dr in &results {
        if dr.vector_id.starts_with("control_") {
            assert_eq!(
                dr.outcome,
                DelegationOutcome::NoEffect,
                "INVARIANT: control {} had false positive under {}",
                dr.vector_id,
                dr.condition
            );
        }
    }
}

#[test]
fn all_vectors_bypass_under_condition_a() {
    let (results, _) = run_delegation_spoofing_matrix();

    let cond_a: Vec<_> = results
        .iter()
        .filter(|r| r.condition == "condition_a" && !r.vector_id.starts_with("control_"))
        .collect();

    assert_eq!(cond_a.len(), 4, "Expected 4 Condition A vector results");

    for dr in &cond_a {
        assert!(
            dr.outcome == DelegationOutcome::ActivationWithTrustUpgrade
                || dr.outcome == DelegationOutcome::ActivationWithSelectionManipulation,
            "{} should bypass under Condition A, got {:?}",
            dr.vector_id,
            dr.outcome
        );
    }
}

#[test]
fn condition_c_blocks_all_vectors() {
    let (results, _) = run_delegation_spoofing_matrix();

    let cond_c: Vec<_> = results
        .iter()
        .filter(|r| r.condition == "condition_c" && !r.vector_id.starts_with("control_"))
        .collect();

    assert_eq!(cond_c.len(), 4);

    for dr in &cond_c {
        assert_eq!(
            dr.outcome,
            DelegationOutcome::ActivationWithCorrectDetection,
            "{} should be detected under Condition C",
            dr.vector_id
        );
    }
}

#[test]
fn v3_identity_spoofing_survives_condition_b() {
    let (results, _) = run_delegation_spoofing_matrix();

    let v3_b: Vec<_> = results
        .iter()
        .filter(|r| r.condition == "condition_b" && r.vector_id == "v3_identity_spoofing")
        .collect();

    assert_eq!(v3_b.len(), 1);
    assert_eq!(
        v3_b[0].outcome,
        DelegationOutcome::ActivationWithTrustUpgrade,
        "V3 should survive Condition B (integrity checks don't verify trust domain)"
    );
}

#[test]
fn full_matrix_structure() {
    let (results, attacks) = run_delegation_spoofing_matrix();
    assert_eq!(
        results.len(),
        21,
        "3 conditions * 7 (4 vectors + 3 controls)"
    );
    assert_eq!(attacks.len(), 21);
}

#[test]
#[ignore]
fn results_output_dump() {
    let (results, _) = run_delegation_spoofing_matrix();

    eprintln!("\n=== DELEGATION SPOOFING EXPERIMENT — FULL RESULTS ===\n");
    eprintln!(
        "{:<30} {:<14} {:<42} {:>5}",
        "VECTOR", "CONDITION", "OUTCOME", "ACCEPTED"
    );
    eprintln!("{:-<30} {:-<14} {:-<42} {:->5}", "", "", "", "");

    for dr in &results {
        eprintln!(
            "{:<30} {:<14} {:<42} {:>5}",
            dr.vector_id,
            dr.condition,
            format!("{:?}", dr.outcome),
            if dr.claim_accepted { "Y" } else { "N" },
        );
    }

    eprintln!("\n=== PER-CONDITION AGGREGATE ===\n");

    for cond in ["condition_a", "condition_b", "condition_c"] {
        let vectors: Vec<_> = results
            .iter()
            .filter(|r| r.condition == cond && !r.vector_id.starts_with("control_"))
            .collect();
        let bypassed = vectors
            .iter()
            .filter(|r| {
                r.outcome == DelegationOutcome::ActivationWithTrustUpgrade
                    || r.outcome == DelegationOutcome::ActivationWithSelectionManipulation
            })
            .count();
        let detected = vectors
            .iter()
            .filter(|r| r.outcome == DelegationOutcome::ActivationWithCorrectDetection)
            .count();

        eprintln!("{}:", cond);
        eprintln!("  Bypassed: {}/{}", bypassed, vectors.len());
        eprintln!("  Detected: {}/{}", detected, vectors.len());
    }

    let fps = results
        .iter()
        .filter(|r| r.vector_id.starts_with("control_") && r.outcome != DelegationOutcome::NoEffect)
        .count();
    eprintln!("\nFPBR: {}/9 ({:.1}%)", fps, (fps as f64 / 9.0) * 100.0);
}
