//! Memory poison invariant tests.
//!
//! Runs the full experiment matrix from assay-sim and validates:
//! 1. Overarching invariant: controls never misclassify
//! 2. Attack vectors activate under Condition A (unprotected)
//! 3. Condition B/C defenses block vectors
//! 4. Full matrix produces expected result count and structure

use assay_sim::attacks::memory_poison::{run_memory_poison_matrix, PoisonOutcome};

#[test]
fn overarching_invariant_no_silent_downgrades_in_controls() {
    let (results, _) = run_memory_poison_matrix();

    for pr in &results {
        if pr.vector_id.starts_with("control_") {
            assert_ne!(
                pr.outcome,
                PoisonOutcome::ActivationWithMisclassification,
                "INVARIANT VIOLATION: control {} produced false positive misclassification",
                pr.vector_id
            );
            assert_ne!(
                pr.outcome,
                PoisonOutcome::ActivationWithPolicyShift,
                "INVARIANT VIOLATION: control {} produced false positive policy shift",
                pr.vector_id
            );
        }
    }
}

#[test]
fn attack_vectors_activate_under_condition_a() {
    let (results, _) = run_memory_poison_matrix();

    let cond_a: Vec<_> = results
        .iter()
        .filter(|r| r.condition == "condition_a" && !r.vector_id.starts_with("control_"))
        .collect();

    assert_eq!(
        cond_a.len(),
        12,
        "Expected 12 Condition A vector results (4 vectors * 3 delays)"
    );

    for pr in &cond_a {
        assert!(
            pr.activation_succeeded,
            "{} should activate under Condition A",
            pr.vector_id
        );
    }
}

#[test]
fn condition_b_blocks_replay_vectors() {
    let (results, _) = run_memory_poison_matrix();

    for pr in results.iter().filter(|r| {
        r.condition == "condition_b"
            && (r.vector_id == "v1_replay_baseline" || r.vector_id == "v2_deny_convergence")
    }) {
        assert_eq!(
            pr.outcome,
            PoisonOutcome::ActivationWithCorrectDetection,
            "{} should be detected under Condition B",
            pr.vector_id
        );
    }
}

#[test]
fn condition_c_blocks_context_envelope() {
    let (results, _) = run_memory_poison_matrix();

    for pr in results
        .iter()
        .filter(|r| r.condition == "condition_c" && r.vector_id == "v3_context_envelope")
    {
        assert_eq!(
            pr.outcome,
            PoisonOutcome::ActivationWithCorrectDetection,
            "V3 should be detected under Condition C"
        );
    }
}

#[test]
fn full_matrix_structure() {
    let (results, attacks) = run_memory_poison_matrix();

    assert_eq!(
        results.len(),
        45,
        "Expected 45 results (3 conditions * 4 vectors * 3 delays + 3 controls * 3 delays)"
    );
    assert_eq!(attacks.len(), 45);
}

#[test]
#[ignore] // Run with: cargo test -p assay-sim --test memory_poison_invariant -- --ignored --nocapture
fn results_output_dump() {
    let (results, _) = run_memory_poison_matrix();

    eprintln!("\n=== MEMORY POISON EXPERIMENT — FULL RESULTS ===\n");
    eprintln!(
        "{:<30} {:<14} {:<40} {:>5} {:>5}",
        "VECTOR", "CONDITION", "OUTCOME", "RET", "ACT"
    );
    eprintln!("{:-<30} {:-<14} {:-<40} {:->5} {:->5}", "", "", "", "", "");

    for pr in &results {
        eprintln!(
            "{:<30} {:<14} {:<40} {:>5} {:>5}",
            pr.vector_id,
            pr.condition,
            format!("{:?}", pr.outcome),
            if pr.poison_retained { "Y" } else { "N" },
            if pr.activation_succeeded { "Y" } else { "N" },
        );
    }

    eprintln!("\n=== PER-CONDITION AGGREGATE ===\n");

    for cond in ["condition_a", "condition_b", "condition_c"] {
        let cond_results: Vec<_> = results
            .iter()
            .filter(|r| r.condition == cond && !r.vector_id.starts_with("control_"))
            .collect();
        let total = cond_results.len();
        let activated = cond_results
            .iter()
            .filter(|r| r.activation_succeeded)
            .count();
        let misclassified = cond_results
            .iter()
            .filter(|r| r.outcome == PoisonOutcome::ActivationWithMisclassification)
            .count();
        let policy_shifted = cond_results
            .iter()
            .filter(|r| r.outcome == PoisonOutcome::ActivationWithPolicyShift)
            .count();
        let detected = cond_results
            .iter()
            .filter(|r| r.outcome == PoisonOutcome::ActivationWithCorrectDetection)
            .count();

        eprintln!("{}:", cond);
        eprintln!(
            "  DASR:  {}/{} ({:.0}%)",
            activated,
            total,
            (activated as f64 / total as f64) * 100.0
        );
        eprintln!("  Misclassifications: {}", misclassified);
        eprintln!("  Policy shifts: {}", policy_shifted);
        eprintln!("  Correctly detected: {}", detected);
    }

    let control_fps = results
        .iter()
        .filter(|r| r.vector_id.starts_with("control_") && r.activation_succeeded)
        .count();
    eprintln!(
        "\nFPBR: {}/9 ({:.1}%)",
        control_fps,
        (control_fps as f64 / 9.0) * 100.0
    );
}
