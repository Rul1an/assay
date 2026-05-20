use serde::{Deserialize, Serialize};

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
}
