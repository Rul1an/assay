use super::super::decision::{reason_codes, DecisionEvent};
use super::types::HandleResult;
use crate::runtime::AuthzReceipt;

pub(super) struct ToolMatchMetadata {
    pub(super) tool_classes: Vec<String>,
    pub(super) matched_tool_classes: Vec<String>,
    pub(super) match_basis: Option<String>,
    pub(super) matched_rule: Option<String>,
}

impl ToolMatchMetadata {
    pub(super) fn new(
        tool_classes: Vec<String>,
        matched_tool_classes: Vec<String>,
        match_basis: Option<String>,
        matched_rule: Option<String>,
    ) -> Self {
        Self {
            tool_classes,
            matched_tool_classes,
            match_basis,
            matched_rule,
        }
    }
}

pub(super) fn error_not_tool_call(event_source: &str, tool_call_id: String) -> HandleResult {
    HandleResult::Error {
        reason_code: reason_codes::S_INTERNAL_ERROR.to_string(),
        reason: "Not a tool call".to_string(),
        decision_event: DecisionEvent::new(
            event_source.to_string(),
            tool_call_id,
            "unknown".to_string(),
        )
        .error(
            reason_codes::S_INTERNAL_ERROR,
            Some("Not a tool call".to_string()),
        ),
    }
}

pub(super) fn deny(
    event_source: &str,
    tool_call_id: String,
    tool_name: String,
    reason_code: &str,
    reason: String,
    tool_match: ToolMatchMetadata,
) -> HandleResult {
    let decision_event = DecisionEvent::new(event_source.to_string(), tool_call_id, tool_name)
        .deny(reason_code, Some(reason.clone()))
        .with_tool_match(
            tool_match.tool_classes,
            tool_match.matched_tool_classes,
            tool_match.match_basis,
            tool_match.matched_rule,
        );

    HandleResult::Deny {
        reason_code: reason_code.to_string(),
        reason,
        decision_event,
    }
}

pub(super) fn allow(
    event_source: &str,
    tool_call_id: String,
    tool_name: String,
    reason_code: &str,
    receipt: Option<AuthzReceipt>,
    tool_match: ToolMatchMetadata,
) -> HandleResult {
    let decision_event = DecisionEvent::new(event_source.to_string(), tool_call_id, tool_name)
        .allow(reason_code)
        .with_tool_match(
            tool_match.tool_classes,
            tool_match.matched_tool_classes,
            tool_match.match_basis,
            tool_match.matched_rule,
        );

    HandleResult::Allow {
        receipt,
        decision_event,
    }
}
