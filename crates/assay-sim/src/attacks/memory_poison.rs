//! Persistent memory poisoning with delayed trigger.
//!
//! Tests whether schema-valid, internally consistent poisoned state entries
//! can survive past injection and later shift policy classification, replay
//! diff buckets, or evidence convergence.
//!
//! 4 attack vectors + 3 benign controls. All deterministic, no LLM calls.

use crate::report::AttackResult;
use serde::Serialize;

#[path = "memory_poison_next/mod.rs"]
mod memory_poison_next;

#[derive(Debug, Clone, Serialize)]
pub struct PoisonResult {
    pub vector_id: String,
    pub condition: String,
    pub phase_a_injected: bool,
    pub phase_b_delay_turns: u32,
    pub phase_c_triggered: bool,
    pub poison_retained: bool,
    pub activation_succeeded: bool,
    pub expected_classification: String,
    pub observed_classification: String,
    pub outcome: PoisonOutcome,
    pub hypothesis_tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PoisonOutcome {
    NoEffect,
    RetainedNoActivation,
    ActivationWithCorrectDetection,
    ActivationWithMisclassification,
    ActivationWithPolicyShift,
}

pub fn vector1_replay_baseline_poisoning(delay_turns: u32) -> (PoisonResult, AttackResult) {
    memory_poison_next::vector1_replay_baseline_poisoning(delay_turns)
}

pub fn vector2_deny_convergence_poisoning(delay_turns: u32) -> (PoisonResult, AttackResult) {
    memory_poison_next::vector2_deny_convergence_poisoning(delay_turns)
}

pub fn vector3_context_envelope_poisoning(delay_turns: u32) -> (PoisonResult, AttackResult) {
    memory_poison_next::vector3_context_envelope_poisoning(delay_turns)
}

pub fn vector4_decay_escape(decay_runs: u32) -> (PoisonResult, AttackResult) {
    memory_poison_next::vector4_decay_escape(decay_runs)
}

pub fn control_b1_run_metadata_recall(delay_turns: u32) -> (PoisonResult, AttackResult) {
    memory_poison_next::control_b1_run_metadata_recall(delay_turns)
}

pub fn control_b2_tool_observation_recall(delay_turns: u32) -> (PoisonResult, AttackResult) {
    memory_poison_next::control_b2_tool_observation_recall(delay_turns)
}

pub fn control_b3_approval_context_recall(delay_turns: u32) -> (PoisonResult, AttackResult) {
    memory_poison_next::control_b3_approval_context_recall(delay_turns)
}

/// Run the complete memory poison experiment matrix across all conditions.
pub fn run_memory_poison_matrix() -> (Vec<PoisonResult>, Vec<AttackResult>) {
    memory_poison_next::run_memory_poison_matrix()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vector1_activates_under_condition_a() {
        let (pr, _) = vector1_replay_baseline_poisoning(1);
        assert!(pr.activation_succeeded);
        assert_eq!(pr.outcome, PoisonOutcome::ActivationWithMisclassification);
    }

    #[test]
    fn vector2_activates_under_condition_a() {
        let (pr, _) = vector2_deny_convergence_poisoning(1);
        // Deny convergence fields are metadata-level; changing them may or may
        // not shift the bucket depending on same_effective_decision_class().
        assert!(pr.phase_c_triggered);
    }

    #[test]
    fn vector3_activates_under_condition_a() {
        let (pr, _) = vector3_context_envelope_poisoning(1);
        assert!(pr.activation_succeeded);
        assert_eq!(pr.outcome, PoisonOutcome::ActivationWithPolicyShift);
    }

    #[test]
    fn vector4_snapshot_diverges() {
        let (pr, _) = vector4_decay_escape(1);
        assert!(pr.activation_succeeded);
    }

    #[test]
    fn controls_produce_no_false_positives() {
        for delay in [1, 2, 3] {
            let (pr1, _) = control_b1_run_metadata_recall(delay);
            assert_eq!(pr1.outcome, PoisonOutcome::NoEffect);

            let (pr2, _) = control_b2_tool_observation_recall(delay);
            assert_eq!(pr2.outcome, PoisonOutcome::NoEffect);

            let (pr3, _) = control_b3_approval_context_recall(delay);
            assert_eq!(pr3.outcome, PoisonOutcome::NoEffect);
        }
    }

    #[test]
    fn full_matrix_runs_without_panic() {
        let (results, attacks) = run_memory_poison_matrix();
        // 3 conditions * 4 vectors * 3 delays + 3 controls * 3 delays = 36 + 9 = 45
        assert_eq!(results.len(), 45);
        assert_eq!(attacks.len(), 45);
    }

    #[test]
    fn condition_b_blocks_v1_and_v2() {
        let (results, _) = run_memory_poison_matrix();
        for pr in results.iter().filter(|r| r.condition == "condition_b") {
            if pr.vector_id == "v1_replay_baseline" || pr.vector_id == "v2_deny_convergence" {
                assert_eq!(
                    pr.outcome,
                    PoisonOutcome::ActivationWithCorrectDetection,
                    "{} should be detected under Condition B",
                    pr.vector_id
                );
            }
        }
    }

    #[test]
    fn condition_c_blocks_v3() {
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
    fn overarching_invariant_controls_never_misclassify() {
        let (results, _) = run_memory_poison_matrix();
        for pr in &results {
            if pr.vector_id.starts_with("control_") {
                assert_ne!(
                    pr.outcome,
                    PoisonOutcome::ActivationWithMisclassification,
                    "control {} had false positive",
                    pr.vector_id
                );
                assert_ne!(
                    pr.outcome,
                    PoisonOutcome::ActivationWithPolicyShift,
                    "control {} had policy shift",
                    pr.vector_id
                );
            }
        }
    }
}
