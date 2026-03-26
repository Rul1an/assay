use super::super::{
    consumer_contract::{project_consumer_contract, DECISION_CONSUMER_CONTRACT_VERSION_V1},
    context_contract::{project_context_contract, DECISION_CONTEXT_CONTRACT_VERSION_V1},
    deny_convergence::{project_deny_convergence, DENY_PRECEDENCE_VERSION_V1},
    outcome_convergence::classify_decision_outcome,
    replay_compat::{project_replay_compat, DECISION_BASIS_VERSION_V1},
};
use super::event_types::{
    Decision, DecisionData, FulfillmentDecisionPath, ObligationOutcome, ObligationOutcomeStatus,
};

const OUTCOME_STAGE_HANDLER: &str = "handler";
const OUTCOME_REASON_CODE_APPLIED: &str = "obligation_applied";
const OUTCOME_REASON_CODE_SKIPPED: &str = "obligation_skipped";
const OUTCOME_REASON_CODE_ERROR: &str = "obligation_error";
const OUTCOME_NORMALIZATION_VERSION_V1: &str = "v1";

fn normalize_obligation_outcome(mut outcome: ObligationOutcome) -> ObligationOutcome {
    if outcome.reason_code.is_none() {
        outcome.reason_code = Some(
            match outcome.status {
                ObligationOutcomeStatus::Applied => OUTCOME_REASON_CODE_APPLIED,
                ObligationOutcomeStatus::Skipped => OUTCOME_REASON_CODE_SKIPPED,
                ObligationOutcomeStatus::Error => OUTCOME_REASON_CODE_ERROR,
            }
            .to_string(),
        );
    }
    if outcome.enforcement_stage.is_none() {
        outcome.enforcement_stage = Some(OUTCOME_STAGE_HANDLER.to_string());
    }
    if outcome.normalization_version.is_none() {
        outcome.normalization_version = Some(OUTCOME_NORMALIZATION_VERSION_V1.to_string());
    }
    outcome
}

fn normalize_obligation_outcomes(outcomes: Vec<ObligationOutcome>) -> Vec<ObligationOutcome> {
    outcomes
        .into_iter()
        .map(normalize_obligation_outcome)
        .collect()
}

fn classify_fulfillment_decision_path(data: &DecisionData) -> FulfillmentDecisionPath {
    match data.decision {
        Decision::Allow => FulfillmentDecisionPath::PolicyAllow,
        Decision::Deny => {
            if data
                .fail_closed
                .as_ref()
                .map(|ctx| ctx.fail_closed_applied)
                .unwrap_or(false)
            {
                FulfillmentDecisionPath::FailClosedDeny
            } else {
                FulfillmentDecisionPath::PolicyDeny
            }
        }
        Decision::Error => FulfillmentDecisionPath::DecisionError,
    }
}

pub(crate) fn refresh_fulfillment_normalization(data: &mut DecisionData) {
    let outcomes = std::mem::take(&mut data.obligation_outcomes);
    data.obligation_outcomes = normalize_obligation_outcomes(outcomes);
    data.obligation_applied_present = Some(
        data.obligation_outcomes
            .iter()
            .any(|outcome| outcome.status == ObligationOutcomeStatus::Applied),
    );
    data.obligation_skipped_present = Some(
        data.obligation_outcomes
            .iter()
            .any(|outcome| outcome.status == ObligationOutcomeStatus::Skipped),
    );
    data.obligation_error_present = Some(
        data.obligation_outcomes
            .iter()
            .any(|outcome| outcome.status == ObligationOutcomeStatus::Error),
    );
    let outcome = classify_decision_outcome(
        data.decision,
        data.reason_code.as_str(),
        data.fail_closed
            .as_ref()
            .map(|ctx| ctx.fail_closed_applied)
            .unwrap_or(false),
        data.obligation_applied_present.unwrap_or(false),
        data.obligation_skipped_present.unwrap_or(false),
        data.obligation_error_present.unwrap_or(false),
    );
    data.decision_outcome_kind = Some(outcome.kind);
    data.decision_origin = Some(outcome.origin);
    data.outcome_compat_state = Some(outcome.compat_state);
    data.fulfillment_decision_path = Some(classify_fulfillment_decision_path(data));
    let fail_closed_applied = data
        .fail_closed
        .as_ref()
        .map(|ctx| ctx.fail_closed_applied)
        .unwrap_or(false);
    let replay_projection = project_replay_compat(
        data.decision_outcome_kind,
        data.decision_origin,
        data.outcome_compat_state,
        data.fulfillment_decision_path,
        data.decision,
    );
    let deny_projection = project_deny_convergence(
        data.decision_outcome_kind,
        data.decision_origin,
        data.fulfillment_decision_path,
        data.decision,
        fail_closed_applied,
        data.reason_code.as_str(),
    );
    data.decision_basis_version = Some(DECISION_BASIS_VERSION_V1.to_string());
    data.compat_fallback_applied = Some(replay_projection.compat_fallback_applied);
    data.classification_source = Some(replay_projection.classification_source);
    data.replay_diff_reason = Some(replay_projection.replay_diff_reason.to_string());
    data.legacy_shape_detected = Some(replay_projection.legacy_shape_detected);
    let consumer_projection = project_consumer_contract(
        data.decision_outcome_kind,
        data.decision_origin,
        data.fulfillment_decision_path,
        data.decision_basis_version.as_deref(),
        data.compat_fallback_applied,
        data.classification_source,
        data.legacy_shape_detected,
    );
    data.decision_consumer_contract_version =
        Some(DECISION_CONSUMER_CONTRACT_VERSION_V1.to_string());
    data.consumer_read_path = Some(consumer_projection.read_path);
    data.consumer_fallback_applied = Some(consumer_projection.fallback_applied);
    data.consumer_payload_state = Some(consumer_projection.payload_state);
    data.required_consumer_fields = consumer_projection.required_consumer_fields;
    let context_projection = project_context_contract(
        data.lane.as_deref(),
        data.principal.as_deref(),
        data.auth_context_summary.as_deref(),
        data.approval_state.as_deref(),
    );
    data.decision_context_contract_version = Some(DECISION_CONTEXT_CONTRACT_VERSION_V1.to_string());
    data.context_payload_state = Some(context_projection.payload_state);
    data.required_context_fields = context_projection.required_context_fields;
    data.missing_context_fields = context_projection.missing_context_fields;
    data.policy_deny = Some(deny_projection.policy_deny);
    data.fail_closed_deny = Some(deny_projection.fail_closed_deny);
    data.enforcement_deny = Some(deny_projection.enforcement_deny);
    data.deny_precedence_version = Some(DENY_PRECEDENCE_VERSION_V1.to_string());
    data.deny_classification_source = Some(deny_projection.classification_source);
    data.deny_legacy_fallback_applied = Some(deny_projection.legacy_fallback_applied);
    data.deny_convergence_reason = Some(deny_projection.deny_convergence_reason.to_string());
}

pub(crate) fn refresh_contract_projections(data: &mut DecisionData) {
    refresh_fulfillment_normalization(data);
}
