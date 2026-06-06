use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct DelegationResult {
    pub vector_id: String,
    pub condition: String,
    pub phase_a_injected: bool,
    pub trigger_activated: bool,
    pub claim_accepted: bool,
    pub expected_trust_level: String,
    pub observed_trust_level: String,
    pub outcome: DelegationOutcome,
    pub hypothesis_tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DelegationOutcome {
    NoEffect,
    RetainedNoActivation,
    ActivationWithCorrectDetection,
    ActivationWithTrustUpgrade,
    ActivationWithSelectionManipulation,
}
