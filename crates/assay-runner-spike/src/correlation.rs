use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const CORRELATION_REPORT_SCHEMA: &str = "assay.runner.correlation_report.v0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorrelationStatus {
    Clean,
    Partial,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindingWindow {
    pub start: String,
    pub end: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorrelationBinding {
    pub tool_call_id: String,
    pub policy_decision: Option<String>,
    pub kernel_event_count: u64,
    pub window: BindingWindow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorrelationReport {
    pub schema: String,
    pub run_id: String,
    pub status: CorrelationStatus,
    pub bindings: Vec<CorrelationBinding>,
    pub ambiguities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CorrelationReportError {
    #[error("correlation report schema must be {CORRELATION_REPORT_SCHEMA}")]
    InvalidSchema,
    #[error("run_id must not be empty")]
    EmptyRunId,
}

impl CorrelationReport {
    pub fn clean(run_id: impl Into<String>) -> Self {
        Self {
            schema: CORRELATION_REPORT_SCHEMA.to_string(),
            run_id: run_id.into(),
            status: CorrelationStatus::Clean,
            bindings: Vec::new(),
            ambiguities: Vec::new(),
        }
    }

    pub fn add_binding(&mut self, binding: CorrelationBinding) {
        self.bindings.push(binding);
    }

    pub fn mark_partial(&mut self, ambiguity: impl Into<String>) {
        if self.status == CorrelationStatus::Clean {
            self.status = CorrelationStatus::Partial;
        }
        self.ambiguities.push(ambiguity.into());
    }

    pub fn mark_failed(&mut self, ambiguity: impl Into<String>) {
        self.status = CorrelationStatus::Failed;
        self.ambiguities.push(ambiguity.into());
    }

    pub fn validate(&self) -> Result<(), CorrelationReportError> {
        if self.schema != CORRELATION_REPORT_SCHEMA {
            return Err(CorrelationReportError::InvalidSchema);
        }
        if self.run_id.is_empty() {
            return Err(CorrelationReportError::EmptyRunId);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adding_partial_ambiguity_changes_clean_report_to_partial() {
        let mut report = CorrelationReport::clean("run_001");

        report.mark_partial("sdk_layer_absent");

        assert_eq!(report.status, CorrelationStatus::Partial);
        assert_eq!(report.ambiguities, vec!["sdk_layer_absent"]);
    }

    #[test]
    fn failed_status_is_sticky() {
        let mut report = CorrelationReport::clean("run_001");

        report.mark_failed("cgroup_correlation_failed");
        report.mark_partial("sdk_layer_absent");

        assert_eq!(report.status, CorrelationStatus::Failed);
        assert_eq!(
            report.ambiguities,
            vec!["cgroup_correlation_failed", "sdk_layer_absent"]
        );
    }

    #[test]
    fn validate_rejects_unexpected_schema() {
        let mut report = CorrelationReport::clean("run_001");
        report.schema = "assay.runner.correlation_report.v_future".to_string();

        assert_eq!(
            report.validate(),
            Err(CorrelationReportError::InvalidSchema)
        );
    }
}
