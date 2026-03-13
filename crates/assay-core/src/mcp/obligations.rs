use super::decision::{ObligationOutcome, ObligationOutcomeStatus};
use super::policy::PolicyObligation;

const OUTCOME_NORMALIZATION_VERSION: &str = "v1";
const STAGE_EXECUTOR: &str = "executor";
const REASON_CODE_LEGACY_WARNING_MAPPED: &str = "legacy_warning_mapped";
const REASON_CODE_VALIDATED_IN_HANDLER: &str = "validated_in_handler";
const REASON_CODE_CONTRACT_ONLY: &str = "contract_only";
const REASON_CODE_UNSUPPORTED_OBLIGATION: &str = "unsupported_obligation_type";

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
                    reason_code: None,
                    enforcement_stage: Some(STAGE_EXECUTOR.to_string()),
                    normalization_version: Some(OUTCOME_NORMALIZATION_VERSION.to_string()),
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
                    reason_code: None,
                    enforcement_stage: Some(STAGE_EXECUTOR.to_string()),
                    normalization_version: Some(OUTCOME_NORMALIZATION_VERSION.to_string()),
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
                    reason_code: Some(REASON_CODE_LEGACY_WARNING_MAPPED.to_string()),
                    enforcement_stage: Some(STAGE_EXECUTOR.to_string()),
                    normalization_version: Some(OUTCOME_NORMALIZATION_VERSION.to_string()),
                }
            }
            "approval_required" => ObligationOutcome {
                obligation_type: "approval_required".to_string(),
                status: ObligationOutcomeStatus::Skipped,
                reason: Some("validated in handler".to_string()),
                reason_code: Some(REASON_CODE_VALIDATED_IN_HANDLER.to_string()),
                enforcement_stage: Some(STAGE_EXECUTOR.to_string()),
                normalization_version: Some(OUTCOME_NORMALIZATION_VERSION.to_string()),
            },
            "restrict_scope" => ObligationOutcome {
                obligation_type: "restrict_scope".to_string(),
                status: ObligationOutcomeStatus::Skipped,
                reason: Some("contract-only in wave29 (no runtime enforcement)".to_string()),
                reason_code: Some(REASON_CODE_CONTRACT_ONLY.to_string()),
                enforcement_stage: Some(STAGE_EXECUTOR.to_string()),
                normalization_version: Some(OUTCOME_NORMALIZATION_VERSION.to_string()),
            },
            "redact_args" => ObligationOutcome {
                obligation_type: "redact_args".to_string(),
                status: ObligationOutcomeStatus::Skipped,
                reason: Some(
                    "contract-only in wave31 (no runtime redaction execution)".to_string(),
                ),
                reason_code: Some(REASON_CODE_CONTRACT_ONLY.to_string()),
                enforcement_stage: Some(STAGE_EXECUTOR.to_string()),
                normalization_version: Some(OUTCOME_NORMALIZATION_VERSION.to_string()),
            },
            other => ObligationOutcome {
                obligation_type: other.to_string(),
                status: ObligationOutcomeStatus::Skipped,
                reason: Some("unsupported obligation type in wave25".to_string()),
                reason_code: Some(REASON_CODE_UNSUPPORTED_OBLIGATION.to_string()),
                enforcement_stage: Some(STAGE_EXECUTOR.to_string()),
                normalization_version: Some(OUTCOME_NORMALIZATION_VERSION.to_string()),
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
        assert!(outcomes[0].reason_code.is_none());
        assert_eq!(outcomes[0].enforcement_stage.as_deref(), Some("executor"));
        assert_eq!(outcomes[0].normalization_version.as_deref(), Some("v1"));
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
        assert_eq!(
            outcomes[0].reason_code.as_deref(),
            Some("legacy_warning_mapped")
        );
        assert_eq!(outcomes[0].enforcement_stage.as_deref(), Some("executor"));
        assert_eq!(outcomes[0].normalization_version.as_deref(), Some("v1"));
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
        assert!(outcomes[0].reason_code.is_none());
        assert_eq!(outcomes[0].enforcement_stage.as_deref(), Some("executor"));
        assert_eq!(outcomes[0].normalization_version.as_deref(), Some("v1"));
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
        assert_eq!(
            outcomes[0].reason_code.as_deref(),
            Some("unsupported_obligation_type")
        );
        assert_eq!(outcomes[0].enforcement_stage.as_deref(), Some("executor"));
        assert_eq!(outcomes[0].normalization_version.as_deref(), Some("v1"));
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
        assert_eq!(outcomes[0].reason_code.as_deref(), Some("contract_only"));
        assert_eq!(outcomes[0].enforcement_stage.as_deref(), Some("executor"));
        assert_eq!(outcomes[0].normalization_version.as_deref(), Some("v1"));
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
        assert_eq!(outcomes[0].reason_code.as_deref(), Some("contract_only"));
        assert_eq!(outcomes[0].enforcement_stage.as_deref(), Some("executor"));
        assert_eq!(outcomes[0].normalization_version.as_deref(), Some("v1"));
    }
}
