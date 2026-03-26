pub(super) mod approval;
pub(super) mod classification;
pub(super) mod fail_closed;
pub(super) mod redaction;
pub(super) mod scope;

pub(super) const OUTCOME_NORMALIZATION_VERSION: &str = "v1";
pub(super) const OUTCOME_STAGE_HANDLER: &str = "handler";
pub(super) const OUTCOME_REASON_VALIDATED_IN_HANDLER: &str = "validated_in_handler";
pub(super) const FAIL_CLOSED_RUNTIME_DEPENDENCY_ERROR: &str =
    "fail_closed_runtime_dependency_error";
pub(super) const DEGRADE_READ_ONLY_RUNTIME_DEPENDENCY_ERROR: &str =
    "degrade_read_only_runtime_dependency_error";
