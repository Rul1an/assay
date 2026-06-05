use super::{
    PolicyDecision, PolicyObligation, RedactArgsContract, RestrictScopeContract,
    TypedPolicyDecision,
};
use serde::{Deserialize, Serialize};

impl PolicyObligation {
    pub fn warning_compat(code: &str, reason: &str) -> Self {
        Self {
            obligation_type: "legacy_warning".to_string(),
            detail: Some(format!("{code}:{reason}")),
            restrict_scope: None,
            redact_args: None,
        }
    }

    pub fn alert(code: &str, reason: &str) -> Self {
        Self {
            obligation_type: "alert".to_string(),
            detail: Some(format!("{code}:{reason}")),
            restrict_scope: None,
            redact_args: None,
        }
    }

    pub fn restrict_scope(contract: RestrictScopeContract, detail: Option<String>) -> Self {
        Self {
            obligation_type: "restrict_scope".to_string(),
            detail,
            restrict_scope: Some(contract),
            redact_args: None,
        }
    }

    pub fn redact_args(contract: RedactArgsContract, detail: Option<String>) -> Self {
        Self {
            obligation_type: "redact_args".to_string(),
            detail,
            restrict_scope: None,
            redact_args: Some(contract),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicyDecisionContract {
    pub decision: TypedPolicyDecision,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub obligations: Vec<PolicyObligation>,
}

impl PolicyDecision {
    pub fn typed_contract(&self) -> PolicyDecisionContract {
        match self {
            Self::Allow => PolicyDecisionContract {
                decision: TypedPolicyDecision::Allow,
                obligations: Vec::new(),
            },
            Self::AllowWithWarning { code, reason, .. } => PolicyDecisionContract {
                decision: TypedPolicyDecision::AllowWithObligations,
                obligations: vec![PolicyObligation::warning_compat(code, reason)],
            },
            Self::Deny { code, reason, .. } if is_alert_deny_code(code) => PolicyDecisionContract {
                decision: TypedPolicyDecision::DenyWithAlert,
                obligations: vec![PolicyObligation::alert(code, reason)],
            },
            Self::Deny { .. } => PolicyDecisionContract {
                decision: TypedPolicyDecision::Deny,
                obligations: Vec::new(),
            },
        }
    }
}

fn is_alert_deny_code(code: &str) -> bool {
    matches!(code, "E_TOOL_DRIFT")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn typed_contract_maps_allow_with_warning_to_legacy_warning_obligation() {
        let decision = PolicyDecision::AllowWithWarning {
            tool: "tool_a".to_string(),
            code: "E_TOOL_UNCONSTRAINED".to_string(),
            reason: "Tool allowed but has no schema".to_string(),
        };

        let contract = decision.typed_contract();
        assert_eq!(contract.decision, TypedPolicyDecision::AllowWithObligations);
        assert_eq!(contract.obligations.len(), 1);
        assert_eq!(contract.obligations[0].obligation_type, "legacy_warning");
    }

    #[test]
    fn typed_contract_maps_tool_drift_to_deny_with_alert_obligation() {
        let decision = PolicyDecision::Deny {
            tool: "tool_a".to_string(),
            code: "E_TOOL_DRIFT".to_string(),
            reason: "Tool drifted".to_string(),
            contract: json!({ "status": "deny" }),
        };

        let contract = decision.typed_contract();
        assert_eq!(contract.decision, TypedPolicyDecision::DenyWithAlert);
        assert_eq!(contract.obligations.len(), 1);
        assert_eq!(contract.obligations[0].obligation_type, "alert");
        assert_eq!(
            contract.obligations[0].detail.as_deref(),
            Some("E_TOOL_DRIFT:Tool drifted")
        );
    }

    #[test]
    fn typed_contract_maps_regular_deny_without_obligations() {
        let decision = PolicyDecision::Deny {
            tool: "tool_a".to_string(),
            code: "E_TOOL_DENIED".to_string(),
            reason: "Denied".to_string(),
            contract: json!({ "status": "deny" }),
        };

        let contract = decision.typed_contract();
        assert_eq!(contract.decision, TypedPolicyDecision::Deny);
        assert!(contract.obligations.is_empty());
    }

    #[test]
    fn restrict_scope_obligation_preserves_typed_shape() {
        let obligation = PolicyObligation::restrict_scope(
            RestrictScopeContract {
                scope_type: "resource".to_string(),
                scope_value: "service/prod".to_string(),
                scope_match_mode: "exact".to_string(),
            },
            Some("shape-only contract".to_string()),
        );

        assert_eq!(obligation.obligation_type, "restrict_scope");
        assert_eq!(
            obligation.restrict_scope,
            Some(RestrictScopeContract {
                scope_type: "resource".to_string(),
                scope_value: "service/prod".to_string(),
                scope_match_mode: "exact".to_string(),
            })
        );
    }

    #[test]
    fn redact_args_obligation_preserves_typed_shape() {
        let obligation = PolicyObligation::redact_args(
            RedactArgsContract {
                redaction_target: "body".to_string(),
                redaction_mode: "mask".to_string(),
                redaction_scope: "request".to_string(),
            },
            Some("shape-only contract".to_string()),
        );

        assert_eq!(obligation.obligation_type, "redact_args");
        assert_eq!(
            obligation.redact_args,
            Some(RedactArgsContract {
                redaction_target: "body".to_string(),
                redaction_mode: "mask".to_string(),
                redaction_scope: "request".to_string(),
            })
        );
    }
}
