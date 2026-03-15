//! Protocol Evidence Interpretation invariant tests.

use assay_sim::attacks::consumer_downgrade::{run_consumer_downgrade_matrix, ConsumerOutcome};

#[test]
fn overarching_invariant_controls_never_downgrade() {
    let (results, _) = run_consumer_downgrade_matrix();
    for cr in &results {
        if cr.vector_id.starts_with("control_") {
            assert_eq!(
                cr.outcome,
                ConsumerOutcome::NoEffect,
                "INVARIANT: control {} false positive under {}",
                cr.vector_id,
                cr.condition
            );
        }
    }
}

#[test]
fn condition_c_blocks_all_vectors() {
    let (results, _) = run_consumer_downgrade_matrix();
    let cond_c: Vec<_> = results
        .iter()
        .filter(|r| r.condition == "condition_c" && !r.vector_id.starts_with("control_"))
        .collect();

    assert_eq!(cond_c.len(), 4);
    for cr in &cond_c {
        assert_ne!(
            cr.outcome,
            ConsumerOutcome::SilentDowngrade,
            "{} should not silently downgrade under C",
            cr.vector_id
        );
        assert_ne!(
            cr.outcome,
            ConsumerOutcome::SilentTrustUpgrade,
            "{} should not silently upgrade trust under C",
            cr.vector_id
        );
    }
}

#[test]
fn condition_a_has_downgrades() {
    let (results, _) = run_consumer_downgrade_matrix();
    let cond_a: Vec<_> = results
        .iter()
        .filter(|r| r.condition == "condition_a" && !r.vector_id.starts_with("control_"))
        .collect();

    assert_eq!(cond_a.len(), 4);
    let downgrades = cond_a
        .iter()
        .filter(|r| r.outcome == ConsumerOutcome::SilentDowngrade)
        .count();
    assert!(
        downgrades >= 2,
        "Expected at least 2 downgrades under A, got {}",
        downgrades
    );
}

#[test]
fn full_matrix_structure() {
    let (results, attacks) = run_consumer_downgrade_matrix();
    assert_eq!(results.len(), 21, "3 conditions * 7");
    assert_eq!(attacks.len(), 21);
}

#[test]
fn ccar_improves_from_a_to_c() {
    let (results, _) = run_consumer_downgrade_matrix();

    let ccar = |cond: &str| {
        let vectors: Vec<_> = results
            .iter()
            .filter(|r| r.condition == cond && !r.vector_id.starts_with("control_"))
            .collect();
        let agree = vectors.iter().filter(|r| !r.downgrade_occurred).count();
        agree as f64 / vectors.len() as f64
    };

    let ccar_a = ccar("condition_a");
    let ccar_c = ccar("condition_c");

    assert!(
        ccar_c > ccar_a,
        "CCAR should improve from A ({:.0}%) to C ({:.0}%)",
        ccar_a * 100.0,
        ccar_c * 100.0
    );
}

#[test]
#[ignore]
fn results_output_dump() {
    let (results, _) = run_consumer_downgrade_matrix();

    eprintln!("\n=== PROTOCOL EVIDENCE INTERPRETATION — FULL RESULTS ===\n");
    eprintln!(
        "{:<28} {:<14} {:<28} {:>5}",
        "VECTOR", "CONDITION", "OUTCOME", "DOWN"
    );
    eprintln!("{:-<28} {:-<14} {:-<28} {:->5}", "", "", "", "");

    for cr in &results {
        eprintln!(
            "{:<28} {:<14} {:<28} {:>5}",
            cr.vector_id,
            cr.condition,
            format!("{:?}", cr.outcome),
            if cr.downgrade_occurred { "Y" } else { "N" },
        );
    }

    eprintln!("\n=== PER-CONDITION AGGREGATE ===\n");

    for cond in ["condition_a", "condition_b", "condition_c"] {
        let vectors: Vec<_> = results
            .iter()
            .filter(|r| r.condition == cond && !r.vector_id.starts_with("control_"))
            .collect();
        let downgrades = vectors.iter().filter(|r| r.downgrade_occurred).count();
        let agree = vectors.iter().filter(|r| !r.downgrade_occurred).count();

        eprintln!("{}:", cond);
        eprintln!("  Downgrades: {}/{}", downgrades, vectors.len());
        eprintln!(
            "  CCAR: {}/{} ({:.0}%)",
            agree,
            vectors.len(),
            (agree as f64 / vectors.len() as f64) * 100.0
        );
    }

    let fps = results
        .iter()
        .filter(|r| r.vector_id.starts_with("control_") && r.outcome != ConsumerOutcome::NoEffect)
        .count();
    eprintln!("\nFPBR: {}/9 ({:.1}%)", fps, (fps as f64 / 9.0) * 100.0);
}
