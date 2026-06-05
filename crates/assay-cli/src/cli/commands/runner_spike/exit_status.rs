#[cfg(target_os = "linux")]
pub(super) fn cgroup_correlation_label(
    status: assay_runner_schema::CgroupCorrelationStatus,
) -> &'static str {
    use assay_runner_schema::CgroupCorrelationStatus;

    match status {
        CgroupCorrelationStatus::Clean => "clean",
        CgroupCorrelationStatus::Partial => "partial",
        CgroupCorrelationStatus::Failed => "failed",
    }
}

#[cfg(target_os = "linux")]
pub(super) fn exit_signal(status: &std::process::ExitStatus) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt;
    status.signal()
}

pub(super) fn exit_status_label(exit_code: Option<i32>, signal: Option<i32>) -> String {
    match (exit_code, signal) {
        (Some(code), _) => format!("exit_code:{code}"),
        (None, Some(signal)) => format!("signal:{signal}"),
        (None, None) => "unknown".to_string(),
    }
}

pub(super) fn exit_status_code(exit_code: Option<i32>, signal: Option<i32>) -> i32 {
    match (exit_code, signal) {
        (Some(code), _) => code,
        (None, Some(signal)) => 128 + signal,
        (None, None) => 1,
    }
}
