use crate::backend::BackendType;
use crate::cli::args::SandboxArgs;
use assay_evidence::types::{
    PayloadSandboxDegraded, SandboxDegradationComponent, SandboxDegradationMode,
    SandboxDegradationReasonCode,
};

pub(super) fn backend_unavailable_degradation(
    args: &SandboxArgs,
    backend: &BackendType,
) -> Option<PayloadSandboxDegraded> {
    if !args.enforce || args.fail_closed || matches!(backend, BackendType::Landlock) {
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
    if !args.enforce || args.fail_closed || !actual_enforcement || compat.is_compatible() {
        return None;
    }

    Some(PayloadSandboxDegraded {
        reason_code: SandboxDegradationReasonCode::PolicyConflict,
        degradation_mode: SandboxDegradationMode::AuditFallback,
        component: SandboxDegradationComponent::Landlock,
        detail: None,
    })
}
