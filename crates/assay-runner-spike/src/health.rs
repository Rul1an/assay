use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const OBSERVATION_HEALTH_SCHEMA: &str = "assay.runner.observation_health.v0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KernelLayerStatus {
    Complete,
    PartialRingbufDrops,
    Absent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyLayerStatus {
    Present,
    Absent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SdkLayerStatus {
    Present,
    SelfReported,
    Absent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CgroupCorrelationStatus {
    Clean,
    Partial,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservationHealth {
    pub schema: String,
    pub run_id: String,
    pub platform: String,
    pub kernel_layer: KernelLayerStatus,
    pub ringbuf_drops: u64,
    pub policy_layer: PolicyLayerStatus,
    pub sdk_layer: SdkLayerStatus,
    pub cgroup_correlation: CgroupCorrelationStatus,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ObservationHealthError {
    #[error("observation health schema must be {OBSERVATION_HEALTH_SCHEMA}")]
    InvalidSchema,
    #[error("run_id must not be empty")]
    EmptyRunId,
    #[error("ringbuf_drops > 0 requires kernel_layer=partial_ringbuf_drops")]
    RingbufDropsRequirePartialKernelLayer,
    #[error("non-Linux platform requires kernel_layer=absent")]
    NonLinuxRequiresAbsentKernelLayer,
    #[error("cgroup_correlation=failed is not a passing Phase 1 run")]
    FailedCgroupCorrelation,
}

impl ObservationHealth {
    pub fn new(run_id: impl Into<String>, platform: impl Into<String>) -> Self {
        Self {
            schema: OBSERVATION_HEALTH_SCHEMA.to_string(),
            run_id: run_id.into(),
            platform: platform.into(),
            kernel_layer: KernelLayerStatus::Complete,
            ringbuf_drops: 0,
            policy_layer: PolicyLayerStatus::Absent,
            sdk_layer: SdkLayerStatus::Absent,
            cgroup_correlation: CgroupCorrelationStatus::Clean,
            notes: Vec::new(),
        }
        .normalized()
    }

    pub fn with_ringbuf_drops(mut self, drops: u64) -> Self {
        self.ringbuf_drops = drops;
        self.apply_rules();
        self
    }

    pub fn with_policy_layer(mut self, policy_layer: PolicyLayerStatus) -> Self {
        self.policy_layer = policy_layer;
        self
    }

    pub fn with_sdk_layer(mut self, sdk_layer: SdkLayerStatus) -> Self {
        self.sdk_layer = sdk_layer;
        self
    }

    pub fn with_agent_shim(mut self, agent_shim: &str) -> Self {
        if agent_shim == "none" {
            self.sdk_layer = SdkLayerStatus::Absent;
        }
        self
    }

    pub fn with_cgroup_correlation(mut self, status: CgroupCorrelationStatus) -> Self {
        self.cgroup_correlation = status;
        self
    }

    pub fn normalized(mut self) -> Self {
        self.apply_rules();
        self
    }

    pub fn validate(&self) -> Result<(), ObservationHealthError> {
        if self.schema != OBSERVATION_HEALTH_SCHEMA {
            return Err(ObservationHealthError::InvalidSchema);
        }
        if self.run_id.is_empty() {
            return Err(ObservationHealthError::EmptyRunId);
        }
        if self.ringbuf_drops > 0 && self.kernel_layer != KernelLayerStatus::PartialRingbufDrops {
            return Err(ObservationHealthError::RingbufDropsRequirePartialKernelLayer);
        }
        if self.platform != "linux" && self.kernel_layer != KernelLayerStatus::Absent {
            return Err(ObservationHealthError::NonLinuxRequiresAbsentKernelLayer);
        }
        if self.cgroup_correlation == CgroupCorrelationStatus::Failed {
            return Err(ObservationHealthError::FailedCgroupCorrelation);
        }
        Ok(())
    }

    fn apply_rules(&mut self) {
        if self.ringbuf_drops > 0 {
            self.kernel_layer = KernelLayerStatus::PartialRingbufDrops;
        }
        if self.platform != "linux" {
            self.kernel_layer = KernelLayerStatus::Absent;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ringbuf_drops_force_partial_kernel_layer() {
        let health = ObservationHealth::new("run_001", "linux").with_ringbuf_drops(2);

        assert_eq!(health.kernel_layer, KernelLayerStatus::PartialRingbufDrops);
        assert_eq!(health.ringbuf_drops, 2);
    }

    #[test]
    fn non_linux_forces_absent_kernel_layer() {
        let health = ObservationHealth::new("run_001", "macos");

        assert_eq!(health.kernel_layer, KernelLayerStatus::Absent);
    }

    #[test]
    fn none_agent_shim_forces_absent_sdk_layer() {
        let health = ObservationHealth::new("run_001", "linux")
            .with_sdk_layer(SdkLayerStatus::SelfReported)
            .with_agent_shim("none");

        assert_eq!(health.sdk_layer, SdkLayerStatus::Absent);
    }

    #[test]
    fn failed_cgroup_correlation_is_not_valid_for_passing_run() {
        let health = ObservationHealth::new("run_001", "linux")
            .with_cgroup_correlation(CgroupCorrelationStatus::Failed);

        assert_eq!(
            health.validate(),
            Err(ObservationHealthError::FailedCgroupCorrelation)
        );
    }

    #[test]
    fn validate_rejects_manual_ringbuf_inconsistency() {
        let mut health = ObservationHealth::new("run_001", "linux");
        health.ringbuf_drops = 1;
        health.kernel_layer = KernelLayerStatus::Complete;

        assert_eq!(
            health.validate(),
            Err(ObservationHealthError::RingbufDropsRequirePartialKernelLayer)
        );
    }

    #[test]
    fn validate_rejects_manual_non_linux_inconsistency() {
        let mut health = ObservationHealth::new("run_001", "macos");
        health.kernel_layer = KernelLayerStatus::Complete;

        assert_eq!(
            health.validate(),
            Err(ObservationHealthError::NonLinuxRequiresAbsentKernelLayer)
        );
    }
}
