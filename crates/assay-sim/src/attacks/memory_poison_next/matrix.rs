use super::conditions::{
    vector1_condition_b, vector2_condition_b, vector3_condition_c, vector4_condition_b,
};
use super::controls::{
    control_b1_run_metadata_recall, control_b2_tool_observation_recall,
    control_b3_approval_context_recall,
};
use super::vectors::{
    vector1_replay_baseline_poisoning, vector2_deny_convergence_poisoning,
    vector3_context_envelope_poisoning, vector4_decay_escape,
};
use crate::attacks::memory_poison::{PoisonOutcome, PoisonResult};
use crate::report::{AttackResult, AttackStatus};
use std::time::Instant;

#[allow(clippy::too_many_arguments)]
pub(in crate::attacks::memory_poison) fn make_result(
    vector: &str,
    condition: &str,
    delay: u32,
    retained: bool,
    activated: bool,
    expected: &str,
    observed: &str,
    outcome: PoisonOutcome,
    tags: Vec<String>,
    start: Instant,
) -> (PoisonResult, AttackResult) {
    let pr = PoisonResult {
        vector_id: vector.to_string(),
        condition: condition.to_string(),
        phase_a_injected: true,
        phase_b_delay_turns: delay,
        phase_c_triggered: true,
        poison_retained: retained,
        activation_succeeded: activated,
        expected_classification: expected.to_string(),
        observed_classification: observed.to_string(),
        outcome: outcome.clone(),
        hypothesis_tags: tags,
    };
    let status = match &outcome {
        PoisonOutcome::ActivationWithMisclassification
        | PoisonOutcome::ActivationWithPolicyShift => AttackStatus::Bypassed,
        PoisonOutcome::ActivationWithCorrectDetection => AttackStatus::Blocked,
        _ => AttackStatus::Passed,
    };
    let ar = AttackResult {
        name: format!("memory_poison.{}.{}.delay_{}", vector, condition, delay),
        status,
        error_class: None,
        error_code: None,
        message: Some(format!(
            "expected={} observed={} outcome={:?}",
            expected, observed, outcome
        )),
        duration_ms: start.elapsed().as_millis() as u64,
    };
    (pr, ar)
}

pub(in crate::attacks::memory_poison) fn run_memory_poison_matrix(
) -> (Vec<PoisonResult>, Vec<AttackResult>) {
    let mut results = Vec::new();
    let mut attacks = Vec::new();

    for delay in [1, 2, 3] {
        let (pr, ar) = vector1_replay_baseline_poisoning(delay);
        results.push(pr);
        attacks.push(ar);
        let (pr, ar) = vector2_deny_convergence_poisoning(delay);
        results.push(pr);
        attacks.push(ar);
        let (pr, ar) = vector3_context_envelope_poisoning(delay);
        results.push(pr);
        attacks.push(ar);
        let (pr, ar) = vector4_decay_escape(delay);
        results.push(pr);
        attacks.push(ar);

        let (pr, ar) = vector1_condition_b(delay);
        results.push(pr);
        attacks.push(ar);
        let (pr, ar) = vector2_condition_b(delay);
        results.push(pr);
        attacks.push(ar);
        let (mut pr, mut ar) = vector3_context_envelope_poisoning(delay);
        pr.condition = "condition_b".to_string();
        ar.name = format!(
            "memory_poison.v3_context_envelope.condition_b.delay_{}",
            delay
        );
        results.push(pr);
        attacks.push(ar);
        let (pr, ar) = vector4_condition_b(delay);
        results.push(pr);
        attacks.push(ar);

        let (mut pr, mut ar) = vector1_condition_b(delay);
        pr.condition = "condition_c".to_string();
        ar.name = format!(
            "memory_poison.v1_replay_baseline.condition_c.delay_{}",
            delay
        );
        results.push(pr);
        attacks.push(ar);
        let (mut pr, mut ar) = vector2_condition_b(delay);
        pr.condition = "condition_c".to_string();
        ar.name = format!(
            "memory_poison.v2_deny_convergence.condition_c.delay_{}",
            delay
        );
        results.push(pr);
        attacks.push(ar);
        let (pr, ar) = vector3_condition_c(delay);
        results.push(pr);
        attacks.push(ar);
        let (mut pr, mut ar) = vector4_condition_b(delay);
        pr.condition = "condition_c".to_string();
        ar.name = format!("memory_poison.v4_decay_escape.condition_c.delay_{}", delay);
        results.push(pr);
        attacks.push(ar);

        let (pr, ar) = control_b1_run_metadata_recall(delay);
        results.push(pr);
        attacks.push(ar);
        let (pr, ar) = control_b2_tool_observation_recall(delay);
        results.push(pr);
        attacks.push(ar);
        let (pr, ar) = control_b3_approval_context_recall(delay);
        results.push(pr);
        attacks.push(ar);
    }

    (results, attacks)
}
