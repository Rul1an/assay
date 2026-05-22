use crate::{run::is_safe_run_id, RunnerSpikeArchive, SdkLayerStatus};
use assay_runner_schema::{SdkLayerEvent, SDK_EVENT_SCHEMA};
use serde_json::Value;
use std::collections::BTreeSet;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdkLayerCapture {
    pub run_id: String,
    pub sdk_layer_ndjson: Vec<u8>,
    pub events: Vec<SdkLayerEvent>,
}

#[derive(Debug, Error)]
pub enum SdkLayerError {
    #[error("sdk layer run_id must not be empty")]
    EmptyRunId,
    #[error("sdk layer run_id may only contain ASCII letters, digits, '_' and '-'")]
    UnsafeRunId,
    #[error("sdk layer run_id mismatch: expected {expected}, found {actual}")]
    RunIdMismatch { expected: String, actual: String },
    #[error("sdk event log did not contain events")]
    EmptySdkLog,
    #[error("invalid sdk event json at line {line}: {source}")]
    InvalidJson {
        line: usize,
        source: serde_json::Error,
    },
    #[error(
        "sdk event line {line} must have schema {SDK_EVENT_SCHEMA}, found {observed_schema:?}"
    )]
    UnexpectedSchema {
        line: usize,
        observed_schema: Option<String>,
    },
    #[error("sdk event line {line} run_id mismatch: expected {expected}, found {actual}")]
    EventRunIdMismatch {
        line: usize,
        expected: String,
        actual: String,
    },
    #[error("sdk event line {line} missing required field {field}")]
    MissingRequiredField { line: usize, field: String },
    #[error("sdk event line {line} event_type {event_type} requires {field}")]
    MissingToolCallField {
        line: usize,
        event_type: String,
        field: &'static str,
    },
    #[error("sdk event line {line} has unsupported event_type {event_type}")]
    UnsupportedEventType { line: usize, event_type: String },
    #[error("sdk event line {line} seq mismatch: expected {expected}, found {actual}")]
    SeqMismatch {
        line: usize,
        expected: u64,
        actual: u64,
    },
    #[error("sdk event serialization failed: {0}")]
    Json(#[from] serde_json::Error),
}

impl SdkLayerCapture {
    pub fn from_sdk_ndjson(
        run_id: impl Into<String>,
        ndjson: &[u8],
    ) -> Result<Self, SdkLayerError> {
        let run_id = run_id.into();
        if run_id.is_empty() {
            return Err(SdkLayerError::EmptyRunId);
        }
        if !is_safe_run_id(&run_id) {
            return Err(SdkLayerError::UnsafeRunId);
        }

        let input = String::from_utf8_lossy(ndjson);
        let mut sdk_layer_ndjson = Vec::new();
        let mut events = Vec::new();

        for (idx, raw) in input.lines().enumerate() {
            let line = idx + 1;
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                continue;
            }
            let value: Value = serde_json::from_str(trimmed)
                .map_err(|source| SdkLayerError::InvalidJson { line, source })?;
            let observed_schema = observed_schema(&value);
            if observed_schema.as_deref() != Some(SDK_EVENT_SCHEMA) {
                return Err(SdkLayerError::UnexpectedSchema {
                    line,
                    observed_schema,
                });
            }

            let event_run_id = required_string(&value, "run_id", line)?;
            if event_run_id != run_id {
                return Err(SdkLayerError::EventRunIdMismatch {
                    line,
                    expected: run_id.clone(),
                    actual: event_run_id,
                });
            }

            let seq = required_u64(&value, "seq", line)?;
            let expected_seq = events.len() as u64;
            if seq != expected_seq {
                return Err(SdkLayerError::SeqMismatch {
                    line,
                    expected: expected_seq,
                    actual: seq,
                });
            }

            let event_type = required_string(&value, "event_type", line)?;
            if !is_supported_event_type(&event_type) {
                return Err(SdkLayerError::UnsupportedEventType { line, event_type });
            }
            let tool_call_id = optional_string(&value, "tool_call_id");
            let tool = optional_string(&value, "tool");
            validate_tool_call_fields(line, &event_type, &tool_call_id, &tool)?;

            let event = SdkLayerEvent {
                schema: SDK_EVENT_SCHEMA.to_string(),
                run_id: run_id.clone(),
                seq,
                event_type,
                source: required_string(&value, "source", line)?,
                sdk_name: optional_string(&value, "sdk_name"),
                sdk_version: optional_string(&value, "sdk_version"),
                tool_call_id,
                tool,
            };
            serde_json::to_writer(&mut sdk_layer_ndjson, &event)?;
            sdk_layer_ndjson.push(b'\n');
            events.push(event);
        }

        if events.is_empty() {
            return Err(SdkLayerError::EmptySdkLog);
        }

        Ok(Self {
            run_id,
            sdk_layer_ndjson,
            events,
        })
    }

    /// Apply this SDK capture to the archive.
    ///
    /// SDK-layer events are self-reported runtime observations. Applying them
    /// may set `sdk_layer=self_reported`, but it must not promote kernel or
    /// policy claims: side-effect evidence remains owned by those layers.
    pub fn apply_to_archive(self, archive: &mut RunnerSpikeArchive) -> Result<(), SdkLayerError> {
        let SdkLayerCapture {
            run_id,
            sdk_layer_ndjson,
            events,
        } = self;

        if archive.run_id != run_id {
            return Err(SdkLayerError::RunIdMismatch {
                expected: archive.run_id.clone(),
                actual: run_id,
            });
        }

        archive.sdk_layer_ndjson = sdk_layer_ndjson;
        archive.observation_health.sdk_layer = SdkLayerStatus::SelfReported;
        archive
            .observation_health
            .notes
            .retain(|note| !note.starts_with("s5_sdk_capture:"));
        let sdk_tool_call_ids = sdk_tool_call_ids(&events);
        mark_sdk_policy_mismatches(archive, &sdk_tool_call_ids);
        archive.observation_health.notes.push(format!(
            "s5_sdk_capture: sdk_events={} sdk_tool_calls={}",
            events.len(),
            sdk_tool_call_ids.len()
        ));
        Ok(())
    }
}

fn required_string(
    value: &Value,
    field: &'static str,
    line: usize,
) -> Result<String, SdkLayerError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| SdkLayerError::MissingRequiredField {
            line,
            field: field.to_string(),
        })
}

fn required_u64(value: &Value, field: &'static str, line: usize) -> Result<u64, SdkLayerError> {
    value
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| SdkLayerError::MissingRequiredField {
            line,
            field: field.to_string(),
        })
}

fn optional_string(value: &Value, field: &'static str) -> Option<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn observed_schema(value: &Value) -> Option<String> {
    value.get("schema").map(|schema| match schema {
        Value::String(schema) => schema.clone(),
        other => other.to_string(),
    })
}

fn is_supported_event_type(event_type: &str) -> bool {
    matches!(
        event_type,
        "tool_call_started" | "tool_call_completed" | "run_finished" | "run_failed"
    )
}

fn validate_tool_call_fields(
    line: usize,
    event_type: &str,
    tool_call_id: &Option<String>,
    tool: &Option<String>,
) -> Result<(), SdkLayerError> {
    if !matches!(event_type, "tool_call_started" | "tool_call_completed") {
        return Ok(());
    }
    if tool_call_id.is_none() {
        return Err(SdkLayerError::MissingToolCallField {
            line,
            event_type: event_type.to_string(),
            field: "tool_call_id",
        });
    }
    if tool.is_none() {
        return Err(SdkLayerError::MissingToolCallField {
            line,
            event_type: event_type.to_string(),
            field: "tool",
        });
    }
    Ok(())
}

fn sdk_tool_call_ids(events: &[SdkLayerEvent]) -> BTreeSet<String> {
    events
        .iter()
        .filter_map(|event| event.tool_call_id.clone())
        .collect()
}

fn mark_sdk_policy_mismatches(
    archive: &mut RunnerSpikeArchive,
    sdk_tool_call_ids: &BTreeSet<String>,
) {
    if archive.policy_layer_ndjson.is_empty() {
        return;
    }

    let policy_binding_ids = archive
        .correlation_report
        .bindings
        .iter()
        .map(|binding| binding.tool_call_id.clone())
        .collect::<BTreeSet<_>>();

    for tool_call_id in sdk_tool_call_ids {
        if !policy_binding_ids.contains(tool_call_id) {
            archive.correlation_report.mark_partial(format!(
                "sdk_tool_call_without_policy_binding:{tool_call_id}"
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{KernelLayerStatus, PolicyLayerStatus};

    const SDK_EVENTS: &[u8] = br#"{"schema":"assay.runner.sdk_event.v0","run_id":"run_001","seq":0,"event_type":"tool_call_started","source":"openai-agents","sdk_name":"@openai/agents","sdk_version":"0.0.0-fixture","tool_call_id":"tc_runner_policy_001","tool":"read_file"}
{"schema":"assay.runner.sdk_event.v0","run_id":"run_001","seq":1,"event_type":"tool_call_completed","source":"openai-agents","sdk_name":"@openai/agents","sdk_version":"0.0.0-fixture","tool_call_id":"tc_runner_policy_001","tool":"read_file"}
"#;

    #[test]
    fn sdk_log_records_self_reported_layer() {
        let capture = SdkLayerCapture::from_sdk_ndjson("run_001", SDK_EVENTS).unwrap();
        let sdk = String::from_utf8(capture.sdk_layer_ndjson.clone()).unwrap();

        assert!(sdk.contains(SDK_EVENT_SCHEMA));
        assert!(sdk.contains("tc_runner_policy_001"));
        assert_eq!(capture.events.len(), 2);
    }

    #[test]
    fn apply_marks_sdk_self_reported_without_promoting_other_layers() {
        let capture = SdkLayerCapture::from_sdk_ndjson("run_001", SDK_EVENTS).unwrap();
        let mut archive = RunnerSpikeArchive::empty("run_001", "linux");

        capture.apply_to_archive(&mut archive).unwrap();

        assert_eq!(
            archive.observation_health.sdk_layer,
            SdkLayerStatus::SelfReported
        );
        assert_eq!(
            archive.observation_health.kernel_layer,
            KernelLayerStatus::Absent
        );
        assert_eq!(
            archive.observation_health.policy_layer,
            PolicyLayerStatus::Absent
        );
        assert!(archive
            .observation_health
            .notes
            .iter()
            .any(|note| note == "s5_sdk_capture: sdk_events=2 sdk_tool_calls=1"));
    }

    #[test]
    fn sdk_policy_matching_tool_call_keeps_existing_correlation_status() {
        let capture = SdkLayerCapture::from_sdk_ndjson("run_001", SDK_EVENTS).unwrap();
        let mut archive = RunnerSpikeArchive::empty("run_001", "linux");
        archive.policy_layer_ndjson = b"{\"schema\":\"assay.runner.policy_event.v0\"}\n".to_vec();
        archive
            .correlation_report
            .add_binding(crate::CorrelationBinding {
                tool_call_id: "tc_runner_policy_001".to_string(),
                policy_decision: Some("allow".to_string()),
                kernel_event_count: 1,
                window: crate::BindingWindow {
                    start: "run_started".to_string(),
                    end: "run_finished".to_string(),
                },
            });

        capture.apply_to_archive(&mut archive).unwrap();

        assert_eq!(
            archive.correlation_report.status,
            crate::CorrelationStatus::Clean
        );
        assert!(archive.correlation_report.ambiguities.is_empty());
    }

    #[test]
    fn sdk_policy_mismatched_tool_call_marks_correlation_partial() {
        let capture = SdkLayerCapture::from_sdk_ndjson("run_001", SDK_EVENTS).unwrap();
        let mut archive = RunnerSpikeArchive::empty("run_001", "linux");
        archive.policy_layer_ndjson = b"{\"schema\":\"assay.runner.policy_event.v0\"}\n".to_vec();
        archive
            .correlation_report
            .add_binding(crate::CorrelationBinding {
                tool_call_id: "tc_different_policy_call".to_string(),
                policy_decision: Some("allow".to_string()),
                kernel_event_count: 1,
                window: crate::BindingWindow {
                    start: "run_started".to_string(),
                    end: "run_finished".to_string(),
                },
            });

        capture.apply_to_archive(&mut archive).unwrap();

        assert_eq!(
            archive.correlation_report.status,
            crate::CorrelationStatus::Partial
        );
        assert!(archive
            .correlation_report
            .ambiguities
            .contains(&"sdk_tool_call_without_policy_binding:tc_runner_policy_001".to_string()));
    }

    #[test]
    fn empty_sdk_log_is_rejected() {
        assert!(matches!(
            SdkLayerCapture::from_sdk_ndjson("run_001", b"\n\n"),
            Err(SdkLayerError::EmptySdkLog)
        ));
    }

    #[test]
    fn unsupported_event_type_is_rejected() {
        let err = SdkLayerCapture::from_sdk_ndjson(
            "run_001",
            br#"{"schema":"assay.runner.sdk_event.v0","run_id":"run_001","seq":0,"event_type":"unknown","source":"fixture"}
"#,
        )
        .unwrap_err();

        assert!(matches!(
            err,
            SdkLayerError::UnsupportedEventType {
                line: 1,
                ref event_type
            } if event_type == "unknown"
        ));
    }

    #[test]
    fn tool_call_events_require_tool_call_id() {
        let err = SdkLayerCapture::from_sdk_ndjson(
            "run_001",
            br#"{"schema":"assay.runner.sdk_event.v0","run_id":"run_001","seq":0,"event_type":"tool_call_started","source":"fixture","tool":"read_file"}
"#,
        )
        .unwrap_err();

        assert!(matches!(
            err,
            SdkLayerError::MissingToolCallField {
                line: 1,
                ref event_type,
                field: "tool_call_id"
            } if event_type == "tool_call_started"
        ));
    }

    #[test]
    fn tool_call_events_require_tool_name() {
        let err = SdkLayerCapture::from_sdk_ndjson(
            "run_001",
            br#"{"schema":"assay.runner.sdk_event.v0","run_id":"run_001","seq":0,"event_type":"tool_call_completed","source":"fixture","tool_call_id":"tc_runner_sdk_001"}
"#,
        )
        .unwrap_err();

        assert!(matches!(
            err,
            SdkLayerError::MissingToolCallField {
                line: 1,
                ref event_type,
                field: "tool"
            } if event_type == "tool_call_completed"
        ));
    }

    #[test]
    fn run_finished_does_not_require_tool_call_fields() {
        let capture = SdkLayerCapture::from_sdk_ndjson(
            "run_001",
            br#"{"schema":"assay.runner.sdk_event.v0","run_id":"run_001","seq":0,"event_type":"run_finished","source":"fixture"}
"#,
        )
        .unwrap();

        assert_eq!(capture.events[0].tool_call_id, None);
        assert_eq!(capture.events[0].tool, None);
    }

    #[test]
    fn seq_must_be_contiguous() {
        let err = SdkLayerCapture::from_sdk_ndjson(
            "run_001",
            br#"{"schema":"assay.runner.sdk_event.v0","run_id":"run_001","seq":1,"event_type":"run_finished","source":"fixture"}
"#,
        )
        .unwrap_err();

        assert!(matches!(
            err,
            SdkLayerError::SeqMismatch {
                line: 1,
                expected: 0,
                actual: 1
            }
        ));
    }

    #[test]
    fn event_run_id_must_match_capture_run_id() {
        let err = SdkLayerCapture::from_sdk_ndjson(
            "run_001",
            br#"{"schema":"assay.runner.sdk_event.v0","run_id":"run_002","seq":0,"event_type":"run_finished","source":"fixture"}
"#,
        )
        .unwrap_err();

        assert!(matches!(
            err,
            SdkLayerError::EventRunIdMismatch {
                line: 1,
                ref expected,
                ref actual
            } if expected == "run_001" && actual == "run_002"
        ));
    }
}
