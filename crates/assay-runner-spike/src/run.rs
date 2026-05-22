use crate::{CgroupCorrelationStatus, KernelLayerStatus, RunnerSpikeArchive};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
use std::process::{Command, ExitStatus};
use std::time::{Duration, Instant};
use thiserror::Error;
use uuid::Uuid;

pub const RUN_EVENT_SCHEMA: &str = "assay.runner.event.v0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunSpec {
    pub run_id: String,
    pub platform: String,
    pub agent_shim: String,
    pub command: Vec<String>,
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RunSpecError {
    #[error("command must not be empty")]
    EmptyCommand,
    #[error("run_id must not be empty")]
    EmptyRunId,
    #[error("run_id must not contain ':'")]
    // Colons are used as prefix separators in policy_decisions values such as
    // "allow:filesystem.read_file"; banning them keeps future join keys simple.
    RunIdContainsColon,
    #[error("run_id may only contain ASCII letters, digits, '_' and '-'")]
    RunIdContainsUnsafeCharacter,
    #[error("unsupported agent shim {0:?}")]
    UnsupportedAgentShim(String),
}

#[derive(Debug, Error)]
pub enum RunExecutionError {
    #[error(transparent)]
    Spec(#[from] RunSpecError),
    #[error("failed to spawn command {command:?}: {source}")]
    Spawn {
        command: String,
        source: std::io::Error,
    },
    #[error("failed to wait for command {command:?}: {source}")]
    Wait {
        command: String,
        source: std::io::Error,
    },
    #[error("failed to serialize runner event: {0}")]
    EventSerialization(#[from] serde_json::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOutcome {
    pub archive: RunnerSpikeArchive,
    pub exit_code: Option<i32>,
    pub signal: Option<i32>,
    pub success: bool,
}

impl RunSpec {
    pub fn new(command: Vec<String>) -> Self {
        Self {
            run_id: generate_run_id(),
            platform: std::env::consts::OS.to_string(),
            agent_shim: "none".to_string(),
            command,
            env: BTreeMap::new(),
        }
    }

    pub fn with_run_id(mut self, run_id: impl Into<String>) -> Self {
        self.run_id = run_id.into();
        self
    }

    pub fn with_platform(mut self, platform: impl Into<String>) -> Self {
        self.platform = platform.into();
        self
    }

    pub fn with_agent_shim(mut self, agent_shim: impl Into<String>) -> Self {
        self.agent_shim = agent_shim.into();
        self
    }

    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    pub fn validate(&self) -> Result<(), RunSpecError> {
        if self.command.is_empty() {
            return Err(RunSpecError::EmptyCommand);
        }
        if self.run_id.is_empty() {
            return Err(RunSpecError::EmptyRunId);
        }
        if self.run_id.contains(':') {
            return Err(RunSpecError::RunIdContainsColon);
        }
        if !is_safe_run_id(&self.run_id) {
            return Err(RunSpecError::RunIdContainsUnsafeCharacter);
        }
        if !matches!(
            self.agent_shim.as_str(),
            "none" | "openai-agents" | "gemini-google-genai"
        ) {
            return Err(RunSpecError::UnsupportedAgentShim(self.agent_shim.clone()));
        }
        Ok(())
    }

    pub fn skeleton_archive(&self) -> Result<RunnerSpikeArchive, RunSpecError> {
        self.validate()?;
        let mut archive = RunnerSpikeArchive::empty(self.run_id.clone(), self.platform.clone());
        archive.observation_health = archive.observation_health.with_agent_shim(&self.agent_shim);
        archive.observation_health.kernel_layer = KernelLayerStatus::Absent;
        archive.observation_health = archive
            .observation_health
            .with_cgroup_correlation(CgroupCorrelationStatus::Partial);
        archive.observation_health.notes.push(
            "contract_only_mode: kernel and cgroup capture not wired in this revision".to_string(),
        );
        Ok(archive)
    }

    pub fn run_contract_only(&self) -> Result<RunOutcome, RunExecutionError> {
        self.validate()?;
        let mut archive = self.skeleton_archive()?;
        let clock = Instant::now();

        self.append_run_started(&mut archive, 0, Duration::ZERO)?;

        let mut child = Command::new(&self.command[0])
            .args(&self.command[1..])
            .envs(&self.env)
            .spawn()
            .map_err(|source| RunExecutionError::Spawn {
                command: self.command[0].clone(),
                source,
            })?;
        let status = child.wait().map_err(|source| RunExecutionError::Wait {
            command: self.command[0].clone(),
            source,
        })?;

        let exit_code = status.code();
        let signal = exit_signal(&status);
        let success = status.success();
        self.append_run_finished(&mut archive, 1, &status, clock.elapsed())?;

        Ok(RunOutcome {
            archive,
            exit_code,
            signal,
            success,
        })
    }

    pub fn append_run_started(
        &self,
        archive: &mut RunnerSpikeArchive,
        seq: u64,
        window_elapsed: Duration,
    ) -> Result<(), RunExecutionError> {
        append_event(
            archive,
            json!({
                "schema": RUN_EVENT_SCHEMA,
                "run_id": &self.run_id,
                "seq": seq,
                "type": "run_started",
                "agent_shim": &self.agent_shim,
                "command": &self.command,
                "env_keys": self.env.keys().collect::<Vec<_>>(),
                "window_elapsed_ms": window_elapsed.as_millis() as u64
            }),
        )?;
        Ok(())
    }

    pub fn append_run_finished(
        &self,
        archive: &mut RunnerSpikeArchive,
        seq: u64,
        status: &ExitStatus,
        window_elapsed: Duration,
    ) -> Result<(), RunExecutionError> {
        append_event(
            archive,
            json!({
                "schema": RUN_EVENT_SCHEMA,
                "run_id": &self.run_id,
                "seq": seq,
                "type": "run_finished",
                "exit_code": status.code(),
                "signal": exit_signal(status),
                "success": status.success(),
                "window_elapsed_ms": window_elapsed.as_millis() as u64
            }),
        )?;
        Ok(())
    }
}

fn generate_run_id() -> String {
    format!("run_{}", Uuid::new_v4().simple())
}

pub(crate) fn is_safe_run_id(run_id: &str) -> bool {
    run_id
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

fn append_event(
    archive: &mut RunnerSpikeArchive,
    event: serde_json::Value,
) -> Result<(), serde_json::Error> {
    serde_json::to_writer(&mut archive.events_ndjson, &event)?;
    archive.events_ndjson.push(b'\n');
    Ok(())
}

#[cfg(unix)]
fn exit_signal(status: &std::process::ExitStatus) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt;
    status.signal()
}

#[cfg(not(unix))]
fn exit_signal(_status: &std::process::ExitStatus) -> Option<i32> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SdkLayerStatus;

    #[test]
    fn generated_run_id_is_stream_safe() {
        let spec = RunSpec::new(vec!["true".to_string()]);

        assert!(spec.run_id.starts_with("run_"));
        assert!(!spec.run_id.contains(':'));
        assert_eq!(spec.platform, std::env::consts::OS);
    }

    #[test]
    fn run_spec_rejects_empty_command() {
        let spec = RunSpec::new(Vec::new());

        assert_eq!(spec.validate(), Err(RunSpecError::EmptyCommand));
    }

    #[test]
    fn run_spec_rejects_colon_run_id() {
        let spec = RunSpec::new(vec!["true".to_string()]).with_run_id("bad:run");

        assert_eq!(spec.validate(), Err(RunSpecError::RunIdContainsColon));
    }

    #[test]
    fn run_spec_rejects_path_like_run_id() {
        let spec = RunSpec::new(vec!["true".to_string()]).with_run_id("../bad");

        assert_eq!(
            spec.validate(),
            Err(RunSpecError::RunIdContainsUnsafeCharacter)
        );
    }

    #[test]
    fn run_spec_rejects_unknown_agent_shim() {
        let spec = RunSpec::new(vec!["true".to_string()]).with_agent_shim("typo-shim");

        assert_eq!(
            spec.validate(),
            Err(RunSpecError::UnsupportedAgentShim("typo-shim".to_string()))
        );
    }

    #[test]
    fn none_shim_skeleton_archive_has_absent_sdk_layer() {
        let archive = RunSpec::new(vec!["true".to_string()])
            .with_run_id("run_001")
            .with_platform("linux")
            .with_agent_shim("none")
            .skeleton_archive()
            .unwrap();

        assert_eq!(archive.run_id, "run_001");
        assert_eq!(archive.observation_health.sdk_layer, SdkLayerStatus::Absent);
    }

    #[test]
    fn sdk_shim_skeleton_archive_stays_absent_until_events_are_consumed() {
        let archive = RunSpec::new(vec!["true".to_string()])
            .with_run_id("run_001")
            .with_platform("linux")
            .with_agent_shim("openai-agents")
            .skeleton_archive()
            .unwrap();

        assert_eq!(archive.observation_health.sdk_layer, SdkLayerStatus::Absent);
    }

    #[test]
    fn gemini_shim_is_accepted_in_allowlist() {
        // The Gemini Python google-genai second-runtime fixture (selected by
        // #1305 via #1306) carries its own bundle metadata via the
        // `gemini-google-genai` shim identifier. Reusing `openai-agents` would
        // mislead any downstream tool that reads `agent_shim` from the bundle.
        // Keep the allowlist explicit; do not relax validation more broadly.
        //
        // This test asserts only the allowlist acceptance plus the skeleton
        // archive's default SDK-layer state (Absent). SDK-layer transition to
        // SelfReported on event application is covered by the SDK-capture
        // tests in src/sdk.rs and src/health.rs, not here.
        let archive = RunSpec::new(vec!["true".to_string()])
            .with_run_id("run_001")
            .with_platform("linux")
            .with_agent_shim("gemini-google-genai")
            .skeleton_archive()
            .unwrap();

        assert_eq!(archive.observation_health.sdk_layer, SdkLayerStatus::Absent);
    }

    #[test]
    fn contract_only_run_records_lifecycle_events_and_exit_code() {
        let outcome = RunSpec::new(vec!["true".to_string()])
            .with_run_id("run_001")
            .with_platform("linux")
            .run_contract_only()
            .unwrap();
        let events = String::from_utf8(outcome.archive.events_ndjson).unwrap();

        assert_eq!(outcome.exit_code, Some(0));
        assert!(outcome.success);
        assert!(events.contains("\"type\":\"run_started\""));
        assert!(events.contains("\"type\":\"run_finished\""));
        assert!(outcome.archive.kernel_layer_ndjson.is_empty());
        assert!(outcome.archive.policy_layer_ndjson.is_empty());
        assert!(outcome.archive.sdk_layer_ndjson.is_empty());
    }

    #[test]
    fn contract_only_bundle_does_not_claim_kernel_observation() {
        let outcome = RunSpec::new(vec!["true".to_string()])
            .with_run_id("run_001")
            .with_platform("linux")
            .run_contract_only()
            .unwrap();

        assert_eq!(
            outcome.archive.observation_health.kernel_layer,
            KernelLayerStatus::Absent
        );
        assert_eq!(
            outcome.archive.observation_health.cgroup_correlation,
            CgroupCorrelationStatus::Partial
        );
        assert!(outcome
            .archive
            .observation_health
            .notes
            .iter()
            .any(|note| note.contains("contract_only_mode")));
    }
}
