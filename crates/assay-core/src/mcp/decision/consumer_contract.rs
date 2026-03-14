use super::{
    DecisionOrigin, DecisionOutcomeKind, FulfillmentDecisionPath, ReplayClassificationSource,
};
use serde::{Deserialize, Serialize};

pub const DECISION_CONSUMER_CONTRACT_VERSION_V1: &str = "wave41_v1";

const REQUIRED_CONSUMER_FIELDS_V1: &[&str] = &[
    "decision",
    "reason_code",
    "decision_outcome_kind",
    "decision_origin",
    "fulfillment_decision_path",
    "decision_basis_version",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsumerReadPath {
    ConvergedDecision,
    CompatibilityMarkers,
    LegacyDecision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsumerPayloadState {
    Converged,
    CompatibilityFallback,
    LegacyBase,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsumerContractProjection {
    pub read_path: ConsumerReadPath,
    pub fallback_applied: bool,
    pub payload_state: ConsumerPayloadState,
    pub required_consumer_fields: Vec<String>,
}

pub fn project_consumer_contract(
    decision_outcome_kind: Option<DecisionOutcomeKind>,
    decision_origin: Option<DecisionOrigin>,
    fulfillment_decision_path: Option<FulfillmentDecisionPath>,
    decision_basis_version: Option<&str>,
    compat_fallback_applied: Option<bool>,
    classification_source: Option<ReplayClassificationSource>,
    legacy_shape_detected: Option<bool>,
) -> ConsumerContractProjection {
    let converged_present = decision_outcome_kind.is_some()
        && decision_origin.is_some()
        && fulfillment_decision_path.is_some();
    let compatibility_present = decision_basis_version.is_some()
        || compat_fallback_applied.is_some()
        || classification_source.is_some()
        || legacy_shape_detected.is_some();

    let read_path = if converged_present {
        ConsumerReadPath::ConvergedDecision
    } else if compatibility_present {
        ConsumerReadPath::CompatibilityMarkers
    } else {
        ConsumerReadPath::LegacyDecision
    };

    let fallback_applied =
        compat_fallback_applied.unwrap_or(read_path != ConsumerReadPath::ConvergedDecision);

    let payload_state = if read_path == ConsumerReadPath::LegacyDecision {
        ConsumerPayloadState::LegacyBase
    } else if fallback_applied || legacy_shape_detected.unwrap_or(false) {
        ConsumerPayloadState::CompatibilityFallback
    } else {
        ConsumerPayloadState::Converged
    };

    ConsumerContractProjection {
        read_path,
        fallback_applied,
        payload_state,
        required_consumer_fields: required_consumer_fields_v1(),
    }
}

pub fn required_consumer_fields_v1() -> Vec<String> {
    REQUIRED_CONSUMER_FIELDS_V1
        .iter()
        .map(|field| (*field).to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_converged_decision_fields() {
        let projection = project_consumer_contract(
            Some(DecisionOutcomeKind::ObligationApplied),
            Some(DecisionOrigin::ObligationExecutor),
            Some(FulfillmentDecisionPath::PolicyAllow),
            Some("wave39_v1"),
            Some(false),
            Some(ReplayClassificationSource::ConvergedOutcome),
            Some(false),
        );

        assert_eq!(projection.read_path, ConsumerReadPath::ConvergedDecision);
        assert!(!projection.fallback_applied);
        assert_eq!(projection.payload_state, ConsumerPayloadState::Converged);
        assert_eq!(
            projection.required_consumer_fields,
            required_consumer_fields_v1()
        );
    }

    #[test]
    fn falls_back_to_compatibility_markers() {
        let projection = project_consumer_contract(
            None,
            None,
            None,
            Some("wave39_v1"),
            Some(true),
            Some(ReplayClassificationSource::FulfillmentPath),
            Some(true),
        );

        assert_eq!(projection.read_path, ConsumerReadPath::CompatibilityMarkers);
        assert!(projection.fallback_applied);
        assert_eq!(
            projection.payload_state,
            ConsumerPayloadState::CompatibilityFallback
        );
    }

    #[test]
    fn falls_back_to_legacy_decision_when_no_markers_exist() {
        let projection = project_consumer_contract(None, None, None, None, None, None, None);

        assert_eq!(projection.read_path, ConsumerReadPath::LegacyDecision);
        assert!(projection.fallback_applied);
        assert_eq!(projection.payload_state, ConsumerPayloadState::LegacyBase);
    }
}
