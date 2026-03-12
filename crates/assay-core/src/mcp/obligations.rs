use super::decision::{ObligationOutcome, ObligationOutcomeStatus};
use super::policy::PolicyObligation;

/// Execute bounded runtime obligations.
///
/// Supported in Wave26:
/// - `log` is applied directly
/// - `alert` is applied as a non-blocking runtime alert signal
/// - `legacy_warning` is mapped to `log` for compatibility
/// - `approval_required` is validated in tool_call_handler (non-blocking here)
/// - `restrict_scope` is emitted as contract/evidence only (non-blocking here)
/// - `redact_args` is emitted as contract/evidence only (non-blocking here)
/// - any other type is emitted as skipped (non-blocking)
pub fn execute_log_only(obligations: &[PolicyObligation], tool: &str) -> Vec<ObligationOutcome> {
    obligations
        .iter()
        .map(|obligation| match obligation.obligation_type.as_str() {
            "log" => {
                tracing::info!(
                    target: "assay::mcp::obligations",
                    tool = %tool,
                    obligation_type = "log",
                    detail = ?obligation.detail,
                    "Applied log obligation"
                );
                ObligationOutcome {
                    obligation_type: "log".to_string(),
                    status: ObligationOutcomeStatus::Applied,
                    reason: None,
                }
            }
            "alert" => {
                tracing::warn!(
                    target: "assay::mcp::obligations",
                    tool = %tool,
                    obligation_type = "alert",
                    detail = ?obligation.detail,
                    "Applied alert obligation"
                );
                ObligationOutcome {
                    obligation_type: "alert".to_string(),
                    status: ObligationOutcomeStatus::Applied,
                    reason: None,
                }
            }
            "legacy_warning" => {
                tracing::warn!(
                    target: "assay::mcp::obligations",
                    tool = %tool,
                    obligation_type = "legacy_warning",
                    detail = ?obligation.detail,
                    "Applied legacy_warning as log obligation"
                );
                ObligationOutcome {
                    obligation_type: "log".to_string(),
                    status: ObligationOutcomeStatus::Applied,
                    reason: Some("mapped from legacy_warning".to_string()),
                }
            }
            "approval_required" => ObligationOutcome {
                obligation_type: "approval_required".to_string(),
                status: ObligationOutcomeStatus::Skipped,
                reason: Some("validated in handler".to_string()),
            },
            "restrict_scope" => ObligationOutcome {
                obligation_type: "restrict_scope".to_string(),
                status: ObligationOutcomeStatus::Skipped,
                reason: Some("contract-only in wave29 (no runtime enforcement)".to_string()),
            },
            "redact_args" => ObligationOutcome {
                obligation_type: "redact_args".to_string(),
                status: ObligationOutcomeStatus::Skipped,
                reason: Some(
                    "contract-only in wave31 (no runtime redaction execution)".to_string(),
                ),
            },
            other => ObligationOutcome {
                obligation_type: other.to_string(),
                status: ObligationOutcomeStatus::Skipped,
                reason: Some("unsupported obligation type in wave25".to_string()),
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execute_log_only_applies_log_obligation() {
        let obligations = vec![PolicyObligation {
            obligation_type: "log".to_string(),
            detail: Some("record event".to_string()),
            restrict_scope: None,
            redact_args: None,
        }];

        let outcomes = execute_log_only(&obligations, "test_tool");
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].obligation_type, "log");
        assert_eq!(outcomes[0].status, ObligationOutcomeStatus::Applied);
        assert!(outcomes[0].reason.is_none());
    }

    #[test]
    fn execute_log_only_maps_legacy_warning_to_log() {
        let obligations = vec![PolicyObligation {
            obligation_type: "legacy_warning".to_string(),
            detail: Some("E_TOOL_UNCONSTRAINED:Tool allowed but has no schema".to_string()),
            restrict_scope: None,
            redact_args: None,
        }];

        let outcomes = execute_log_only(&obligations, "test_tool");
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].obligation_type, "log");
        assert_eq!(outcomes[0].status, ObligationOutcomeStatus::Applied);
        assert_eq!(
            outcomes[0].reason.as_deref(),
            Some("mapped from legacy_warning")
        );
    }

    #[test]
    fn execute_log_only_applies_alert_obligation() {
        let obligations = vec![PolicyObligation {
            obligation_type: "alert".to_string(),
            detail: Some("notify-monitor".to_string()),
            restrict_scope: None,
            redact_args: None,
        }];

        let outcomes = execute_log_only(&obligations, "test_tool");
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].obligation_type, "alert");
        assert_eq!(outcomes[0].status, ObligationOutcomeStatus::Applied);
        assert!(outcomes[0].reason.is_none());
    }

    #[test]
    fn execute_log_only_skips_unsupported_obligation_type() {
        let obligations = vec![PolicyObligation {
            obligation_type: "custom_blocking_gate".to_string(),
            detail: None,
            restrict_scope: None,
            redact_args: None,
        }];

        let outcomes = execute_log_only(&obligations, "test_tool");
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].obligation_type, "custom_blocking_gate");
        assert_eq!(outcomes[0].status, ObligationOutcomeStatus::Skipped);
        assert_eq!(
            outcomes[0].reason.as_deref(),
            Some("unsupported obligation type in wave25")
        );
    }

    #[test]
    fn execute_log_only_marks_restrict_scope_as_contract_only() {
        let obligations = vec![PolicyObligation {
            obligation_type: "restrict_scope".to_string(),
            detail: Some("shape-only".to_string()),
            restrict_scope: Some(crate::mcp::policy::RestrictScopeContract {
                scope_type: "resource".to_string(),
                scope_value: "service/prod".to_string(),
                scope_match_mode: "exact".to_string(),
            }),
            redact_args: None,
        }];

        let outcomes = execute_log_only(&obligations, "test_tool");
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].obligation_type, "restrict_scope");
        assert_eq!(outcomes[0].status, ObligationOutcomeStatus::Skipped);
        assert_eq!(
            outcomes[0].reason.as_deref(),
            Some("contract-only in wave29 (no runtime enforcement)")
        );
    }

    #[test]
    fn execute_log_only_marks_redact_args_as_contract_only() {
        let obligations = vec![PolicyObligation {
            obligation_type: "redact_args".to_string(),
            detail: Some("shape-only".to_string()),
            restrict_scope: None,
            redact_args: Some(crate::mcp::policy::RedactArgsContract {
                redaction_target: "body".to_string(),
                redaction_mode: "mask".to_string(),
                redaction_scope: "request".to_string(),
            }),
        }];

        let outcomes = execute_log_only(&obligations, "test_tool");
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].obligation_type, "redact_args");
        assert_eq!(outcomes[0].status, ObligationOutcomeStatus::Skipped);
        assert_eq!(
            outcomes[0].reason.as_deref(),
            Some("contract-only in wave31 (no runtime redaction execution)")
        );
    }
}
