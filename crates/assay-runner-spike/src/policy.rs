use crate::{
    run::is_safe_run_id, BindingWindow, CapabilitySurface, CorrelationBinding, PolicyLayerStatus,
    RunnerSpikeArchive,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

pub const POLICY_EVENT_SCHEMA: &str = "assay.runner.policy_event.v0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyLayerEvent {
    pub schema: String,
    pub run_id: String,
    pub seq: u64,
    pub source_event_type: String,
    pub source: String,
    pub tool_call_id: String,
    pub tool: String,
    pub decision: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyLayerCapture {
    pub run_id: String,
    pub policy_layer_ndjson: Vec<u8>,
    pub capability_surface: CapabilitySurface,
    pub events: Vec<PolicyLayerEvent>,
}

#[derive(Debug, Error)]
pub enum PolicyLayerError {
    #[error("policy layer run_id must not be empty")]
    EmptyRunId,
    #[error("policy layer run_id may only contain ASCII letters, digits, '_' and '-'")]
    UnsafeRunId,
    #[error("policy layer run_id mismatch: expected {expected}, found {actual}")]
    RunIdMismatch { expected: String, actual: String },
    #[error("policy decision log did not contain decision events")]
    EmptyDecisionLog,
    #[error("invalid policy decision json at line {line}: {source}")]
    InvalidJson {
        line: usize,
        source: serde_json::Error,
    },
    #[error(
        "policy decision line {line} must have type assay.tool.decision, found {observed_type:?}"
    )]
    UnexpectedEventType {
        line: usize,
        observed_type: Option<String>,
    },
    #[error("policy decision line {line} missing required field {field}")]
    MissingRequiredField { line: usize, field: String },
    #[error("invalid capability surface: {0}")]
    CapabilitySurface(#[from] assay_runner_schema::CapabilitySurfaceError),
    #[error("policy event serialization failed: {0}")]
    Json(#[from] serde_json::Error),
}

impl PolicyLayerCapture {
    pub fn from_decision_ndjson(
        run_id: impl Into<String>,
        ndjson: &[u8],
    ) -> Result<Self, PolicyLayerError> {
        let run_id = run_id.into();
        if run_id.is_empty() {
            return Err(PolicyLayerError::EmptyRunId);
        }
        if !is_safe_run_id(&run_id) {
            return Err(PolicyLayerError::UnsafeRunId);
        }

        let input = String::from_utf8_lossy(ndjson);
        let mut policy_layer_ndjson = Vec::new();
        let mut capability_surface = CapabilitySurface::new(run_id.clone());
        let mut events = Vec::new();

        for (idx, raw) in input.lines().enumerate() {
            let line = idx + 1;
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                continue;
            }
            let value: Value = serde_json::from_str(trimmed)
                .map_err(|source| PolicyLayerError::InvalidJson { line, source })?;
            let observed_type = observed_type(&value);
            if observed_type.as_deref() != Some("assay.tool.decision") {
                return Err(PolicyLayerError::UnexpectedEventType {
                    line,
                    observed_type,
                });
            }
            let data = value
                .get("data")
                .ok_or_else(|| PolicyLayerError::MissingRequiredField {
                    line,
                    field: "data".to_string(),
                })?;
            let source = required_string(&value, "source", line)?;
            let tool = required_data_string(data, "tool", line)?;
            let tool_call_id = required_data_string(data, "tool_call_id", line)?;
            let decision = required_data_string(data, "decision", line)?;

            capability_surface.add_mcp_tool(tool.clone());
            capability_surface.add_policy_decision(format!("{decision}:{tool}"));

            let event = PolicyLayerEvent {
                schema: POLICY_EVENT_SCHEMA.to_string(),
                run_id: run_id.clone(),
                seq: events.len() as u64,
                source_event_type: "assay.tool.decision".to_string(),
                source,
                tool_call_id,
                tool,
                decision,
            };
            serde_json::to_writer(&mut policy_layer_ndjson, &event)?;
            policy_layer_ndjson.push(b'\n');
            events.push(event);
        }

        if events.is_empty() {
            return Err(PolicyLayerError::EmptyDecisionLog);
        }

        Ok(Self {
            run_id,
            policy_layer_ndjson,
            capability_surface,
            events,
        })
    }

    /// Apply this policy capture to the archive.
    ///
    /// Must be called after any kernel-layer capture has been applied:
    /// `kernel_event_count` is read from `archive.kernel_layer_ndjson` at apply
    /// time. Calling this on an archive with no kernel events marks the
    /// correlation report partial with `policy_events_without_kernel_events`.
    pub fn apply_to_archive(
        self,
        archive: &mut RunnerSpikeArchive,
    ) -> Result<(), PolicyLayerError> {
        let PolicyLayerCapture {
            run_id,
            policy_layer_ndjson,
            capability_surface,
            events,
        } = self;

        if archive.run_id != run_id {
            return Err(PolicyLayerError::RunIdMismatch {
                expected: archive.run_id.clone(),
                actual: run_id,
            });
        }

        archive.policy_layer_ndjson = policy_layer_ndjson;
        archive.capability_surface.merge_from(&capability_surface)?;
        archive.observation_health.policy_layer = PolicyLayerStatus::Present;
        archive
            .observation_health
            .notes
            .retain(|note| !note.starts_with("s4_policy_capture:"));
        archive.observation_health.notes.push(format!(
            "s4_policy_capture: decision_events={}",
            events.len()
        ));

        let kernel_event_count = archive
            .kernel_layer_ndjson
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .count() as u64;
        if kernel_event_count == 0 {
            archive
                .correlation_report
                .mark_partial("policy_events_without_kernel_events");
        }

        for event in events {
            archive.correlation_report.add_binding(CorrelationBinding {
                tool_call_id: event.tool_call_id,
                policy_decision: Some(event.decision),
                kernel_event_count,
                window: BindingWindow {
                    start: "run_started".to_string(),
                    end: "run_finished".to_string(),
                },
            });
        }
        Ok(())
    }
}

fn required_string(
    value: &Value,
    field: &'static str,
    line: usize,
) -> Result<String, PolicyLayerError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| PolicyLayerError::MissingRequiredField {
            line,
            field: field.to_string(),
        })
}

fn required_data_string(
    data: &Value,
    field: &'static str,
    line: usize,
) -> Result<String, PolicyLayerError> {
    data.get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| PolicyLayerError::MissingRequiredField {
            line,
            field: format!("data.{field}"),
        })
}

fn observed_type(value: &Value) -> Option<String> {
    value.get("type").map(|event_type| match event_type {
        Value::String(event_type) => event_type.clone(),
        other => other.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CgroupCorrelationStatus, KernelLayerStatus};

    const DECISION: &[u8] = br#"{"specversion":"1.0","id":"evt_decision_001","type":"assay.tool.decision","source":"assay://runner-spike/run_001","time":"2026-05-20T00:00:00Z","data":{"tool":"read_file","decision":"allow","reason_code":"P_TOOL_ALLOWED","tool_call_id":"tc_runner_policy_001"}}
"#;

    #[test]
    fn decision_log_records_policy_layer_and_surface() {
        let capture = PolicyLayerCapture::from_decision_ndjson("run_001", DECISION).unwrap();
        let policy = String::from_utf8(capture.policy_layer_ndjson.clone()).unwrap();

        assert!(policy.contains(POLICY_EVENT_SCHEMA));
        assert!(policy.contains("tc_runner_policy_001"));
        assert!(capture.capability_surface.mcp_tools.contains("read_file"));
        assert!(capture
            .capability_surface
            .policy_decisions
            .contains("allow:read_file"));
    }

    #[test]
    fn apply_marks_policy_present_and_adds_binding() {
        let capture = PolicyLayerCapture::from_decision_ndjson("run_001", DECISION).unwrap();
        let mut archive = RunnerSpikeArchive::empty("run_001", "linux");
        archive.observation_health.kernel_layer = KernelLayerStatus::Complete;
        archive.observation_health.cgroup_correlation = CgroupCorrelationStatus::Clean;
        archive.kernel_layer_ndjson = b"{\"schema\":\"assay.runner.kernel_event.v0\"}\n".to_vec();

        capture.apply_to_archive(&mut archive).unwrap();

        assert_eq!(
            archive.observation_health.policy_layer,
            PolicyLayerStatus::Present
        );
        assert_eq!(archive.correlation_report.bindings.len(), 1);
        assert_eq!(
            archive.correlation_report.bindings[0].tool_call_id,
            "tc_runner_policy_001"
        );
        assert_eq!(archive.correlation_report.bindings[0].kernel_event_count, 1);
    }

    #[test]
    fn empty_decision_log_is_rejected() {
        assert!(matches!(
            PolicyLayerCapture::from_decision_ndjson("run_001", b"\n"),
            Err(PolicyLayerError::EmptyDecisionLog)
        ));
    }

    #[test]
    fn unexpected_event_type_reports_observed_type() {
        let err = PolicyLayerCapture::from_decision_ndjson(
            "run_001",
            br#"{"type":"assay.other.event","source":"assay://test","data":{}}
"#,
        )
        .unwrap_err();

        assert!(matches!(
            err,
            PolicyLayerError::UnexpectedEventType {
                line: 1,
                observed_type: Some(ref observed)
            } if observed == "assay.other.event"
        ));
        assert!(err.to_string().contains("assay.other.event"));
    }

    #[test]
    fn missing_top_level_source_is_not_reported_as_data_source() {
        let err = PolicyLayerCapture::from_decision_ndjson(
            "run_001",
            br#"{"type":"assay.tool.decision","data":{"tool":"read_file","decision":"allow","tool_call_id":"tc_runner_policy_001"}}
"#,
        )
        .unwrap_err();

        assert!(matches!(
            err,
            PolicyLayerError::MissingRequiredField {
                line: 1,
                ref field
            } if field == "source"
        ));
        assert!(err.to_string().contains("missing required field source"));
        assert!(!err.to_string().contains("data.source"));
    }

    #[test]
    fn policy_without_kernel_events_marks_correlation_partial() {
        let capture = PolicyLayerCapture::from_decision_ndjson("run_001", DECISION).unwrap();
        let mut archive = RunnerSpikeArchive::empty("run_001", "linux");

        capture.apply_to_archive(&mut archive).unwrap();

        assert_eq!(
            archive.correlation_report.status,
            crate::CorrelationStatus::Partial
        );
        assert!(archive
            .correlation_report
            .ambiguities
            .iter()
            .any(|ambiguity| ambiguity == "policy_events_without_kernel_events"));
    }
}
