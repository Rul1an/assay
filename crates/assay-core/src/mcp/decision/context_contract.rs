use serde::{Deserialize, Serialize};

pub const DECISION_CONTEXT_CONTRACT_VERSION_V1: &str = "wave42_v1";

const REQUIRED_CONTEXT_FIELDS_V1: &[&str] = &[
    "lane",
    "principal",
    "auth_context_summary",
    "approval_state",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextPayloadState {
    CompleteEnvelope,
    PartialEnvelope,
    AbsentEnvelope,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextContractProjection {
    pub payload_state: ContextPayloadState,
    pub required_context_fields: Vec<String>,
    pub missing_context_fields: Vec<String>,
}

pub fn project_context_contract(
    lane: Option<&str>,
    principal: Option<&str>,
    auth_context_summary: Option<&str>,
    approval_state: Option<&str>,
) -> ContextContractProjection {
    let fields = [
        ("lane", lane),
        ("principal", principal),
        ("auth_context_summary", auth_context_summary),
        ("approval_state", approval_state),
    ];

    let present = fields.iter().filter(|(_, value)| value.is_some()).count();
    let payload_state = match present {
        0 => ContextPayloadState::AbsentEnvelope,
        n if n == fields.len() => ContextPayloadState::CompleteEnvelope,
        _ => ContextPayloadState::PartialEnvelope,
    };

    let missing_context_fields = fields
        .iter()
        .filter_map(|(field, value)| value.is_none().then_some((*field).to_string()))
        .collect();

    ContextContractProjection {
        payload_state,
        required_context_fields: required_context_fields_v1(),
        missing_context_fields,
    }
}

pub fn required_context_fields_v1() -> Vec<String> {
    REQUIRED_CONTEXT_FIELDS_V1
        .iter()
        .map(|field| (*field).to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_complete_context_envelope() {
        let projection = project_context_contract(
            Some("lane-prod"),
            Some("alice@example.com"),
            Some("aud=deploy scopes=tool:deploy"),
            Some("approved"),
        );

        assert_eq!(
            projection.payload_state,
            ContextPayloadState::CompleteEnvelope
        );
        assert!(projection.missing_context_fields.is_empty());
        assert_eq!(
            projection.required_context_fields,
            required_context_fields_v1()
        );
    }

    #[test]
    fn classifies_partial_context_envelope() {
        let projection =
            project_context_contract(Some("lane-prod"), None, Some("aud=deploy"), None);

        assert_eq!(
            projection.payload_state,
            ContextPayloadState::PartialEnvelope
        );
        assert_eq!(
            projection.missing_context_fields,
            vec!["principal".to_string(), "approval_state".to_string()]
        );
    }

    #[test]
    fn classifies_absent_context_envelope() {
        let projection = project_context_contract(None, None, None, None);

        assert_eq!(
            projection.payload_state,
            ContextPayloadState::AbsentEnvelope
        );
        assert_eq!(
            projection.missing_context_fields,
            required_context_fields_v1()
        );
    }
}
