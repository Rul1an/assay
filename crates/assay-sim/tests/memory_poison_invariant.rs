//! Memory poison invariant tests.
//!
//! Runs the full experiment matrix from assay-sim and validates:
//! 1. Overarching invariant: controls never misclassify
//! 2. Attack vectors activate under Condition A (unprotected)
//! 3. Full matrix produces expected result count and structure

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

    let v1_results: Vec<_> = results
        .iter()
        .filter(|r| r.vector_id == "v1_replay_baseline")
        .collect();
    let v3_results: Vec<_> = results
        .iter()
        .filter(|r| r.vector_id == "v3_context_envelope")
        .collect();
    let v4_results: Vec<_> = results
        .iter()
        .filter(|r| r.vector_id == "v4_decay_escape")
        .collect();

    assert!(
        v1_results.iter().all(|r| r.activation_succeeded),
        "V1 should activate under Condition A"
    );
    assert!(
        v3_results.iter().all(|r| r.activation_succeeded),
        "V3 should activate under Condition A"
    );
    assert!(
        v4_results.iter().all(|r| r.activation_succeeded),
        "V4 should activate under Condition A"
    );
}

#[test]
fn full_matrix_structure() {
    let (results, attacks) = run_memory_poison_matrix();

    assert_eq!(
        results.len(),
        21,
        "Expected 21 results (7 per delay * 3 delays)"
    );
    assert_eq!(attacks.len(), 21);

    let vectors: Vec<_> = results
        .iter()
        .filter(|r| !r.vector_id.starts_with("control_"))
        .collect();
    let controls: Vec<_> = results
        .iter()
        .filter(|r| r.vector_id.starts_with("control_"))
        .collect();

    assert_eq!(
        vectors.len(),
        12,
        "Expected 12 vector results (4 vectors * 3 delays)"
    );
    assert_eq!(
        controls.len(),
        9,
        "Expected 9 control results (3 controls * 3 delays)"
    );
}

#[test]
fn results_output_dump() {
    let (results, _) = run_memory_poison_matrix();

    eprintln!("\n=== MEMORY POISON EXPERIMENT RESULTS ===\n");
    eprintln!(
        "{:<30} {:<14} {:>5} {:>5} {:<40}",
        "VECTOR", "OUTCOME", "RET", "ACT", "CLASSIFICATION"
    );
    eprintln!("{:-<30} {:-<14} {:->5} {:->5} {:-<40}", "", "", "", "", "");

    for pr in &results {
        eprintln!(
            "{:<30} {:<14} {:>5} {:>5} expected={} observed={}",
            pr.vector_id,
            format!("{:?}", pr.outcome),
            if pr.poison_retained { "Y" } else { "N" },
            if pr.activation_succeeded { "Y" } else { "N" },
            pr.expected_classification,
            pr.observed_classification,
        );
    }

    eprintln!("\n=== AGGREGATE ===\n");

    let total_vectors = results
        .iter()
        .filter(|r| !r.vector_id.starts_with("control_"))
        .count();
    let activated = results
        .iter()
        .filter(|r| !r.vector_id.starts_with("control_") && r.activation_succeeded)
        .count();
    let misclassified = results
        .iter()
        .filter(|r| r.outcome == PoisonOutcome::ActivationWithMisclassification)
        .count();
    let policy_shifted = results
        .iter()
        .filter(|r| r.outcome == PoisonOutcome::ActivationWithPolicyShift)
        .count();
    let control_fps = results
        .iter()
        .filter(|r| r.vector_id.starts_with("control_") && r.activation_succeeded)
        .count();

    eprintln!("Attack vectors: {}/{} activated", activated, total_vectors);
    eprintln!("Misclassifications: {}", misclassified);
    eprintln!("Policy shifts: {}", policy_shifted);
    eprintln!("Control false positives: {}/9", control_fps);
    eprintln!("FPBR: {:.1}%", (control_fps as f64 / 9.0) * 100.0);
}
