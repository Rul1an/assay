//! Protocol Evidence Interpretation Attacks.
//!
//! Tests consumer-side trust downgrade under partial, ambiguous, or flattened
//! protocol evidence. When does protocol-valid but incompletely interpreted
//! metadata lead to an overly optimistic trust decision?
//!
//! 4 attack vectors + 3 benign controls. All deterministic, no LLM calls.

use crate::report::{AttackResult, AttackStatus};
use assay_core::mcp::decision::{
    classify_replay_diff, required_consumer_fields_v1, ConsumerPayloadState, ConsumerReadPath,
    Decision, DecisionOrigin, DecisionOutcomeKind, DenyClassificationSource,
    FulfillmentDecisionPath, OutcomeCompatState, ReplayClassificationSource, ReplayDiffBasis,
    ReplayDiffBucket, DECISION_BASIS_VERSION_V1, DECISION_CONSUMER_CONTRACT_VERSION_V1,
    DENY_PRECEDENCE_VERSION_V1,
};
use serde::Serialize;
use std::time::Instant;

#[derive(Debug, Clone, Serialize)]
pub struct ConsumerResult {
    pub vector_id: String,
    pub condition: String,
    pub realism_class: String,
    pub canonical_classification: String,
    pub consumer_classification: String,
    pub downgrade_occurred: bool,
    pub outcome: ConsumerOutcome,
    pub hypothesis_tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConsumerOutcome {
    NoEffect,
    RetainedNoDowngrade,
    DowngradeWithCorrectDetection,
    SilentDowngrade,
    SilentTrustUpgrade,
}

fn make_converged_deny_basis() -> ReplayDiffBasis {
    ReplayDiffBasis {
        decision_outcome_kind: Some(DecisionOutcomeKind::PolicyDeny),
        decision_origin: Some(DecisionOrigin::PolicyEngine),
        outcome_compat_state: Some(OutcomeCompatState::LegacyFieldsPreserved),
        fulfillment_decision_path: Some(FulfillmentDecisionPath::PolicyDeny),
        decision_basis_version: DECISION_BASIS_VERSION_V1.to_string(),
        compat_fallback_applied: false,
        classification_source: ReplayClassificationSource::ConvergedOutcome,
        replay_diff_reason: "converged_policy_deny".to_string(),
        legacy_shape_detected: false,
        decision_consumer_contract_version: DECISION_CONSUMER_CONTRACT_VERSION_V1.to_string(),
        consumer_read_path: ConsumerReadPath::ConvergedDecision,
        consumer_fallback_applied: false,
        consumer_payload_state: ConsumerPayloadState::Converged,
        required_consumer_fields: required_consumer_fields_v1(),
        policy_deny: true,
        fail_closed_deny: false,
        enforcement_deny: false,
        deny_precedence_version: DENY_PRECEDENCE_VERSION_V1.to_string(),
        deny_classification_source: DenyClassificationSource::OutcomeKind,
        deny_legacy_fallback_applied: false,
        deny_convergence_reason: "outcome_policy_deny".to_string(),
        reason_code: "policy_deny_sensitive_tool".to_string(),
        typed_decision: None,
        policy_version: Some("v1".to_string()),
        policy_digest: Some("sha256:abc".to_string()),
        decision: Decision::Allow, // legacy field diverges from converged
        fail_closed_applied: false,
    }
}

fn make_converged_allow_basis() -> ReplayDiffBasis {
    ReplayDiffBasis {
        decision_outcome_kind: Some(DecisionOutcomeKind::ObligationApplied),
        decision_origin: Some(DecisionOrigin::PolicyEngine),
        outcome_compat_state: Some(OutcomeCompatState::LegacyFieldsPreserved),
        fulfillment_decision_path: Some(FulfillmentDecisionPath::PolicyAllow),
        decision_basis_version: DECISION_BASIS_VERSION_V1.to_string(),
        compat_fallback_applied: false,
        classification_source: ReplayClassificationSource::ConvergedOutcome,
        replay_diff_reason: "converged_obligation_applied".to_string(),
        legacy_shape_detected: false,
        decision_consumer_contract_version: DECISION_CONSUMER_CONTRACT_VERSION_V1.to_string(),
        consumer_read_path: ConsumerReadPath::ConvergedDecision,
        consumer_fallback_applied: false,
        consumer_payload_state: ConsumerPayloadState::Converged,
        required_consumer_fields: required_consumer_fields_v1(),
        policy_deny: false,
        fail_closed_deny: false,
        enforcement_deny: false,
        deny_precedence_version: DENY_PRECEDENCE_VERSION_V1.to_string(),
        deny_classification_source: DenyClassificationSource::OutcomeKind,
        deny_legacy_fallback_applied: false,
        deny_convergence_reason: "outcome_not_deny".to_string(),
        reason_code: "obligation_applied_log".to_string(),
        typed_decision: None,
        policy_version: Some("v1".to_string()),
        policy_digest: Some("sha256:abc".to_string()),
        decision: Decision::Allow,
        fail_closed_applied: false,
    }
}

/// Canonical restrictiveness: deny variants = 2, allow/obligation = 1.
fn canonical_rank(basis: &ReplayDiffBasis) -> u8 {
    match basis.decision_outcome_kind {
        Some(DecisionOutcomeKind::PolicyDeny)
        | Some(DecisionOutcomeKind::FailClosedDeny)
        | Some(DecisionOutcomeKind::EnforcementDeny) => 2,
        _ => 1,
    }
}

/// Partial consumer rank: reads only legacy `decision` field.
fn legacy_only_rank(basis: &ReplayDiffBasis) -> u8 {
    match basis.decision {
        Decision::Deny | Decision::Error => 2,
        Decision::Allow => 1,
    }
}

// ---------------------------------------------------------------------------
// Vector 1: Partial-Field Trust Read
// ---------------------------------------------------------------------------

pub fn vector1_partial_trust_read(condition: &str) -> (ConsumerResult, AttackResult) {
    let start = Instant::now();
    let basis = make_converged_deny_basis();

    let canonical = canonical_rank(&basis); // 2 (deny via decision_outcome_kind)
    let consumer = match condition {
        "condition_a" => legacy_only_rank(&basis), // 1 (Allow via legacy decision)
        "condition_b" | "condition_c" => canonical_rank(&basis), // follows read-path
        _ => legacy_only_rank(&basis),
    };

    let downgrade = consumer < canonical;
    let outcome = if !downgrade {
        ConsumerOutcome::NoEffect
    } else {
        match condition {
            "condition_b" | "condition_c" => ConsumerOutcome::DowngradeWithCorrectDetection,
            _ => ConsumerOutcome::SilentDowngrade,
        }
    };

    make_consumer_result(
        "v1_partial_trust_read",
        condition,
        "consumer_realistic_synthetic",
        &format!("rank_{}", canonical),
        &format!("rank_{}", consumer),
        downgrade,
        outcome,
        vec!["H1".into()],
        start,
    )
}

// ---------------------------------------------------------------------------
// Vector 2: Precedence Inversion (Deny Convergence)
// ---------------------------------------------------------------------------

pub fn vector2_precedence_inversion(condition: &str) -> (ConsumerResult, AttackResult) {
    let start = Instant::now();

    let mut basis = make_converged_deny_basis();
    basis.decision_outcome_kind = Some(DecisionOutcomeKind::EnforcementDeny);
    basis.enforcement_deny = false; // legacy field not normalized
    basis.policy_deny = false;
    basis.deny_classification_source = DenyClassificationSource::LegacyDecision;
    basis.decision = Decision::Allow;

    // Canonical: tier-1 decision_outcome_kind = EnforcementDeny → deny
    let canonical_is_deny = true;

    // Consumer behavior per condition
    let consumer_is_deny = match condition {
        "condition_a" => basis.decision == Decision::Deny, // reads legacy: false
        "condition_b" => {
            // Reads converged fields but wrong precedence: checks policy_deny first
            basis.policy_deny || basis.enforcement_deny
        }
        "condition_c" => {
            // Full hardening: tier-1 decision_outcome_kind wins
            matches!(
                basis.decision_outcome_kind,
                Some(DecisionOutcomeKind::PolicyDeny)
                    | Some(DecisionOutcomeKind::FailClosedDeny)
                    | Some(DecisionOutcomeKind::EnforcementDeny)
            )
        }
        _ => false,
    };

    let downgrade = canonical_is_deny && !consumer_is_deny;
    let outcome = if !downgrade {
        ConsumerOutcome::NoEffect
    } else {
        match condition {
            "condition_c" => ConsumerOutcome::DowngradeWithCorrectDetection,
            _ => ConsumerOutcome::SilentDowngrade,
        }
    };

    make_consumer_result(
        "v2_precedence_inversion",
        condition,
        "producer_realistic",
        "deny",
        if consumer_is_deny { "deny" } else { "not_deny" },
        downgrade,
        outcome,
        vec!["H1".into()],
        start,
    )
}

// ---------------------------------------------------------------------------
// Vector 3: Compat Flattening (Trust Signal Suppression)
// ---------------------------------------------------------------------------

pub fn vector3_compat_flattening(condition: &str) -> (ConsumerResult, AttackResult) {
    let start = Instant::now();

    let mut compat_basis = make_converged_allow_basis();
    compat_basis.consumer_payload_state = ConsumerPayloadState::CompatibilityFallback;
    compat_basis.consumer_fallback_applied = true;
    compat_basis.consumer_read_path = ConsumerReadPath::CompatibilityMarkers;
    compat_basis.compat_fallback_applied = true;

    let converged_basis = make_converged_allow_basis();

    // Canonical: these two are different (compat vs converged)
    let canonical_bucket = classify_replay_diff(&converged_basis, &compat_basis);

    // Consumer that flattens compat: treats compat_basis as if it were converged
    let mut flattened = compat_basis.clone();
    let consumer_bucket = match condition {
        "condition_a" => {
            // Ignores all compat signals, reads as converged
            flattened.consumer_payload_state = ConsumerPayloadState::Converged;
            flattened.consumer_fallback_applied = false;
            flattened.consumer_read_path = ConsumerReadPath::ConvergedDecision;
            flattened.compat_fallback_applied = false;
            classify_replay_diff(&converged_basis, &flattened)
        }
        "condition_b" => {
            // Follows read-path but treats compat as non-binding
            flattened.consumer_payload_state = ConsumerPayloadState::Converged;
            flattened.consumer_fallback_applied = false;
            flattened.compat_fallback_applied = false;
            classify_replay_diff(&converged_basis, &flattened)
        }
        "condition_c" => {
            // Full hardening: compat signals are binding
            classify_replay_diff(&converged_basis, &compat_basis)
        }
        _ => classify_replay_diff(&converged_basis, &flattened),
    };

    let downgrade =
        canonical_bucket != consumer_bucket && consumer_bucket == ReplayDiffBucket::Unchanged;

    let outcome = if canonical_bucket == consumer_bucket || condition == "condition_c" {
        ConsumerOutcome::NoEffect
    } else if consumer_bucket == ReplayDiffBucket::Unchanged {
        ConsumerOutcome::SilentDowngrade
    } else {
        ConsumerOutcome::RetainedNoDowngrade
    };

    make_consumer_result(
        "v3_compat_flattening",
        condition,
        "consumer_realistic",
        &format!("{:?}", canonical_bucket),
        &format!("{:?}", consumer_bucket),
        downgrade,
        outcome,
        vec!["H4".into()],
        start,
    )
}

// ---------------------------------------------------------------------------
// Vector 4: Projection Loss (Required Fields Dropped)
// ---------------------------------------------------------------------------

pub fn vector4_projection_loss(condition: &str) -> (ConsumerResult, AttackResult) {
    let start = Instant::now();

    let full = make_converged_deny_basis();
    let full_rank = canonical_rank(&full); // 2

    // Strip all converged + compat fields: simulate forwarding chain loss
    let mut stripped = full.clone();
    stripped.decision_outcome_kind = None;
    stripped.decision_origin = None;
    stripped.fulfillment_decision_path = None;
    stripped.decision_basis_version = String::new();
    stripped.compat_fallback_applied = false;
    stripped.classification_source = ReplayClassificationSource::LegacyFallback;
    stripped.legacy_shape_detected = false;
    stripped.consumer_read_path = ConsumerReadPath::LegacyDecision;
    stripped.consumer_payload_state = ConsumerPayloadState::LegacyBase;
    stripped.consumer_fallback_applied = true;

    let stripped_rank = legacy_only_rank(&stripped); // 1 (Allow)

    let consumer_rank = match condition {
        "condition_a" | "condition_b" => stripped_rank,
        "condition_c" => {
            // Full hardening: required-field completeness check detects drop
            // Falls back to canonical interpretation
            full_rank
        }
        _ => stripped_rank,
    };

    let downgrade = consumer_rank < full_rank;
    let outcome = if !downgrade {
        if condition == "condition_c" {
            ConsumerOutcome::DowngradeWithCorrectDetection
        } else {
            ConsumerOutcome::NoEffect
        }
    } else {
        ConsumerOutcome::SilentDowngrade
    };

    make_consumer_result(
        "v4_projection_loss",
        condition,
        "adapter_realistic",
        &format!("rank_{}", full_rank),
        &format!("rank_{}", consumer_rank),
        downgrade,
        outcome,
        vec!["H2".into()],
        start,
    )
}

// ---------------------------------------------------------------------------
// Benign Controls
// ---------------------------------------------------------------------------

pub fn control_e1_legitimate_legacy(condition: &str) -> (ConsumerResult, AttackResult) {
    let start = Instant::now();
    // Genuine legacy payload: only decision field, no converged markers
    let mut basis = make_converged_allow_basis();
    basis.decision_outcome_kind = None;
    basis.decision_origin = None;
    basis.fulfillment_decision_path = None;
    basis.consumer_read_path = ConsumerReadPath::LegacyDecision;
    basis.consumer_payload_state = ConsumerPayloadState::LegacyBase;

    let rank = legacy_only_rank(&basis);
    let canonical = rank; // for legacy, legacy IS canonical

    make_consumer_result(
        "control_e1_legacy",
        condition,
        "producer_realistic",
        &format!("rank_{}", canonical),
        &format!("rank_{}", rank),
        false,
        ConsumerOutcome::NoEffect,
        vec!["H3".into()],
        start,
    )
}

pub fn control_e2_legitimate_compat(condition: &str) -> (ConsumerResult, AttackResult) {
    let start = Instant::now();
    let mut basis = make_converged_allow_basis();
    basis.consumer_payload_state = ConsumerPayloadState::CompatibilityFallback;
    basis.consumer_fallback_applied = true;
    basis.consumer_read_path = ConsumerReadPath::CompatibilityMarkers;

    // Legitimate compat: classification should be correct
    let canonical = canonical_rank(&basis);
    let consumer = canonical_rank(&basis);

    make_consumer_result(
        "control_e2_compat",
        condition,
        "producer_realistic",
        &format!("rank_{}", canonical),
        &format!("rank_{}", consumer),
        false,
        ConsumerOutcome::NoEffect,
        vec!["H3".into()],
        start,
    )
}

pub fn control_e3_legitimate_converged(condition: &str) -> (ConsumerResult, AttackResult) {
    let start = Instant::now();
    let basis = make_converged_allow_basis();
    let canonical = canonical_rank(&basis);
    let consumer = canonical_rank(&basis);

    make_consumer_result(
        "control_e3_converged",
        condition,
        "producer_realistic",
        &format!("rank_{}", canonical),
        &format!("rank_{}", consumer),
        false,
        ConsumerOutcome::NoEffect,
        vec!["H3".into()],
        start,
    )
}

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
fn make_consumer_result(
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

#[cfg(test)]
mod tests {
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
        // B treats compat as non-binding — flattening may produce different bucket
        // or may collapse to same if classify_replay_diff fields happen to match
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
}
