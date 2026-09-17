use crate::backend::BackendType;
use crate::cli::args::SandboxArgs;
use assay_evidence::types::{
    PayloadSandboxDegraded, SandboxDegradationComponent, SandboxDegradationMode,
    SandboxDegradationReasonCode,
};

/// `--fail-closed`, and `--enforce` without `--allow-audit-fallback`, refuse
/// rather than continue in audit. One predicate for the no-backend guard, the
/// policy-conflict guard, and the degradation-event guards.
pub(super) fn refuses_when_unenforceable(args: &SandboxArgs) -> bool {
    args.fail_closed || (args.enforce && !args.allow_audit_fallback)
}

pub(super) fn backend_unavailable_degradation(
    args: &SandboxArgs,
    backend: &BackendType,
) -> Option<PayloadSandboxDegraded> {
    if refuses_when_unenforceable(args) || !args.enforce || matches!(backend, BackendType::Landlock)
    {
        return None;
    }

    Some(PayloadSandboxDegraded {
        reason_code: SandboxDegradationReasonCode::BackendUnavailable,
        degradation_mode: SandboxDegradationMode::AuditFallback,
        component: SandboxDegradationComponent::Landlock,
        detail: None,
    })
}

pub(super) fn policy_conflict_degradation(
    args: &SandboxArgs,
    actual_enforcement: bool,
    compat: &crate::landlock_check::LandlockCompatReport,
) -> Option<PayloadSandboxDegraded> {
    if refuses_when_unenforceable(args)
        || !args.enforce
        || !actual_enforcement
        || compat.is_compatible()
    {
        return None;
    }

    Some(PayloadSandboxDegraded {
        reason_code: SandboxDegradationReasonCode::PolicyConflict,
        degradation_mode: SandboxDegradationMode::AuditFallback,
        component: SandboxDegradationComponent::Landlock,
        detail: None,
    })
}
