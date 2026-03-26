use super::super::super::policy::{
    FailClosedContext, FailClosedMode, FailClosedTrigger, ToolRiskClass,
};
use super::super::emit;
use super::{DEGRADE_READ_ONLY_RUNTIME_DEPENDENCY_ERROR, FAIL_CLOSED_RUNTIME_DEPENDENCY_ERROR};
use crate::runtime::OperationClass;

pub(in crate::mcp::tool_call_handler) fn seed_fail_closed_context(
    tool_match: &mut emit::ToolMatchMetadata,
    op: OperationClass,
) {
    let tool_risk_class = match op {
        OperationClass::Read => ToolRiskClass::LowRiskRead,
        OperationClass::Write | OperationClass::Commit => ToolRiskClass::HighRisk,
    };
    let fail_closed_mode = match tool_risk_class {
        ToolRiskClass::LowRiskRead => FailClosedMode::DegradeReadOnly,
        ToolRiskClass::HighRisk | ToolRiskClass::Default => FailClosedMode::FailClosed,
    };
    tool_match.fail_closed = Some(FailClosedContext {
        tool_risk_class,
        fail_closed_mode,
        fail_closed_trigger: None,
        fail_closed_applied: false,
        fail_closed_error_code: None,
    });
}

pub(in crate::mcp::tool_call_handler) fn runtime_dependency_error_code(
    tool_match: &emit::ToolMatchMetadata,
) -> &'static str {
    match tool_match
        .fail_closed
        .as_ref()
        .map(|ctx| ctx.fail_closed_mode)
        .unwrap_or(FailClosedMode::FailClosed)
    {
        FailClosedMode::DegradeReadOnly => DEGRADE_READ_ONLY_RUNTIME_DEPENDENCY_ERROR,
        FailClosedMode::FailClosed | FailClosedMode::FailSafeAllow => {
            FAIL_CLOSED_RUNTIME_DEPENDENCY_ERROR
        }
    }
}

pub(in crate::mcp::tool_call_handler) fn mark_fail_closed(
    tool_match: &mut emit::ToolMatchMetadata,
    trigger: FailClosedTrigger,
    error_code: String,
) {
    if let Some(ctx) = tool_match.fail_closed.as_mut() {
        ctx.fail_closed_trigger = Some(trigger);
        ctx.fail_closed_applied = true;
        ctx.fail_closed_error_code = Some(error_code);
    }
}
