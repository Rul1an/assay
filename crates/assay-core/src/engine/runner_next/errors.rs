use crate::errors::{try_map_error, RunError, RunErrorKind};
use crate::model::{EvalConfig, LlmResponse, TestCase, TestResultRow, TestStatus};
use crate::on_error::{ErrorPolicy, ErrorPolicyResult};

pub(crate) fn error_row_and_output_impl(
    cfg: &EvalConfig,
    tc: &TestCase,
    e: anyhow::Error,
    error_policy: ErrorPolicy,
) -> (TestResultRow, LlmResponse) {
    let msg = if let Some(diag) = try_map_error(&e) {
        diag.to_string()
    } else {
        e.to_string()
    };

    let policy_result = error_policy.apply_to_error(&e);
    let (status, final_msg, applied_policy) = match policy_result {
        ErrorPolicyResult::Blocked { reason } => (TestStatus::Error, reason, ErrorPolicy::Block),
        ErrorPolicyResult::Allowed { warning } => {
            crate::on_error::log_fail_safe(&warning, None);
            (TestStatus::AllowedOnError, warning, ErrorPolicy::Allow)
        }
    };
    let run_error = e
        .downcast_ref::<RunError>()
        .cloned()
        .unwrap_or_else(|| RunError::from_anyhow(&e));
    let run_error_kind = match &run_error.kind {
        RunErrorKind::TraceNotFound => "trace_not_found",
        RunErrorKind::MissingConfig => "missing_config",
        RunErrorKind::ConfigParse => "config_parse",
        RunErrorKind::InvalidArgs => "invalid_args",
        RunErrorKind::ProviderRateLimit => "provider_rate_limit",
        RunErrorKind::ProviderTimeout => "provider_timeout",
        RunErrorKind::ProviderServer => "provider_server",
        RunErrorKind::Network => "network",
        RunErrorKind::JudgeUnavailable => "judge_unavailable",
        RunErrorKind::Other => "other",
    };

    (
        TestResultRow {
            test_id: tc.id.clone(),
            status,
            score: None,
            cached: false,
            message: final_msg,
            details: serde_json::json!({
                "error": msg,
                "policy_applied": applied_policy,
                "run_error_kind": run_error_kind,
                "run_error_legacy": run_error.legacy_classified,
                "run_error": {
                    "path": run_error.path,
                    "status": run_error.status,
                    "provider": run_error.provider,
                    "detail": run_error.detail
                }
            }),
            duration_ms: None,
            fingerprint: None,
            skip_reason: None,
            attempts: None,
            error_policy_applied: Some(applied_policy),
        },
        LlmResponse {
            text: "".into(),
            provider: "error".into(),
            model: cfg.model.clone(),
            cached: false,
            meta: serde_json::json!({}),
        },
    )
}
