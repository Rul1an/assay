use super::controls::{
    control_e1_legitimate_legacy, control_e2_legitimate_compat, control_e3_legitimate_converged,
};
use super::vectors::{
    vector1_partial_trust_read, vector2_precedence_inversion, vector3_compat_flattening,
    vector4_projection_loss,
};
use super::{ConsumerOutcome, ConsumerResult};
use crate::report::{AttackResult, AttackStatus};
use std::time::Instant;

// ---------------------------------------------------------------------------
// Full matrix runner
// ---------------------------------------------------------------------------

pub fn run_consumer_downgrade_matrix() -> (Vec<ConsumerResult>, Vec<AttackResult>) {
    let mut results = Vec::new();
    let mut attacks = Vec::new();

    for condition in ["condition_a", "condition_b", "condition_c"] {
        for vector_fn in [
            vector1_partial_trust_read,
            vector2_precedence_inversion,
            vector3_compat_flattening,
            vector4_projection_loss,
        ] {
            let (cr, ar) = vector_fn(condition);
            results.push(cr);
            attacks.push(ar);
        }

        for control_fn in [
            control_e1_legitimate_legacy,
            control_e2_legitimate_compat,
            control_e3_legitimate_converged,
        ] {
            let (cr, ar) = control_fn(condition);
            results.push(cr);
            attacks.push(ar);
        }
    }

    (results, attacks)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn make_consumer_result(
    vector: &str,
    condition: &str,
    realism: &str,
    canonical: &str,
    consumer: &str,
    downgrade: bool,
    outcome: ConsumerOutcome,
    tags: Vec<String>,
    start: Instant,
) -> (ConsumerResult, AttackResult) {
    let cr = ConsumerResult {
        vector_id: vector.to_string(),
        condition: condition.to_string(),
        realism_class: realism.to_string(),
        canonical_classification: canonical.to_string(),
        consumer_classification: consumer.to_string(),
        downgrade_occurred: downgrade,
        outcome: outcome.clone(),
        hypothesis_tags: tags,
    };
    let status = match &outcome {
        ConsumerOutcome::SilentDowngrade | ConsumerOutcome::SilentTrustUpgrade => {
            AttackStatus::Bypassed
        }
        ConsumerOutcome::DowngradeWithCorrectDetection => AttackStatus::Blocked,
        _ => AttackStatus::Passed,
    };
    let ar = AttackResult {
        name: format!("consumer.{}.{}", vector, condition),
        status,
        error_class: None,
        error_code: None,
        message: Some(format!(
            "canonical={} consumer={} downgrade={} outcome={:?}",
            canonical, consumer, downgrade, outcome
        )),
        duration_ms: start.elapsed().as_millis() as u64,
    };
    (cr, ar)
}
