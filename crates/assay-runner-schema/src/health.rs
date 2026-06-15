use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const OBSERVATION_HEALTH_SCHEMA: &str = "assay.runner.observation_health.v0";

/// Capture-side secret redaction summary (ADR-034). Value-free: it states that redaction happened,
/// of which rule class and in which field, plus the redaction-domain key id, never the matched value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Redaction {
    /// `shape_and_flag` | `shape_only` | `disabled_unsafe`.
    pub mode: String,
    pub redacted_count: u64,
    pub by_rule: BTreeMap<String, u64>,
    pub by_field: BTreeMap<String, u64>,
    /// `host_local` | `ephemeral`.
    pub key_scope: String,
    /// Non-reversible digest of the redaction key (`hmac-sha256:<hex>`); never the key itself.
    pub key_id: String,
}

/// Schema id for the standalone, versioned capture-side redaction receipt (MCP01a).
pub const REDACTION_RECEIPT_SCHEMA: &str = "assay.redaction_receipt.v0";

/// Standalone, versioned capture-side redaction receipt: the ADR-034 `Redaction` summary promoted to
/// a first-class carrier so it can be emitted and consumed on its own, not only nested in
/// observation_health. Capture evidence ONLY — it proves redaction ran at capture, NOT that rendered
/// sinks are safe (that is `assay.render_safety_conformance.v0`). MCP01 Strong is not claimed from
/// this receipt alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactionReceipt {
    pub schema: String,
    #[serde(flatten)]
    pub redaction: Redaction,
}

/// Expectation-aware consumer reading of a redaction receipt (the value a reviewer gates on).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RedactionReceiptStatus {
    /// `shape_and_flag` | `shape_only`: capture redaction was active.
    Active,
    /// `disabled_unsafe`: redaction was off — hard not-clean, the evidence may carry raw credentials.
    Blocked,
    /// An unrecognised mode: not reviewed, never silently clean.
    Unsupported,
}

impl RedactionReceipt {
    pub fn new(redaction: Redaction) -> Self {
        Self {
            schema: REDACTION_RECEIPT_SCHEMA.to_string(),
            redaction,
        }
    }

    /// The gate reading. `disabled_unsafe` blocks; a recognised active mode is active; anything else
    /// is unsupported (never clean). A *missing* receipt is the consumer's concern (incomplete), not
    /// representable here.
    pub fn status(&self) -> RedactionReceiptStatus {
        match self.redaction.mode.as_str() {
            "shape_and_flag" | "shape_only" => RedactionReceiptStatus::Active,
            "disabled_unsafe" => RedactionReceiptStatus::Blocked,
            _ => RedactionReceiptStatus::Unsupported,
        }
    }
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NetworkProtocolCoverageStatus {
    #[default]
    Unknown,
    Absent,
    ConnectOnly,
    DatagramPeerObserved,
    ConnectAndDatagramPeerObserved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NetworkEndpointClaimScope {
    #[default]
    Unknown,
    NotApplicable,
    DiagnosticOnly,
    PeerSet,
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
    #[serde(default)]
    pub network_protocol_coverage: NetworkProtocolCoverageStatus,
    #[serde(default)]
    pub network_endpoint_claim_scope: NetworkEndpointClaimScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redaction: Option<Redaction>,
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
            kernel_layer: KernelLayerStatus::Absent,
            ringbuf_drops: 0,
            policy_layer: PolicyLayerStatus::Absent,
            sdk_layer: SdkLayerStatus::Absent,
            cgroup_correlation: CgroupCorrelationStatus::Partial,
            network_protocol_coverage: NetworkProtocolCoverageStatus::Absent,
            network_endpoint_claim_scope: NetworkEndpointClaimScope::NotApplicable,
            redaction: None,
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
        if self.platform != "linux" || self.kernel_layer == KernelLayerStatus::Absent {
            self.network_protocol_coverage = NetworkProtocolCoverageStatus::Absent;
            self.network_endpoint_claim_scope = NetworkEndpointClaimScope::NotApplicable;
        }
        // The SDK layer is intentionally not derived from the declared shim.
        // A shim can crash before emitting events, which legitimately leaves
        // sdk_layer=absent even when the requested shim was not "none".
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_redaction(mode: &str) -> Redaction {
        Redaction {
            mode: mode.to_string(),
            redacted_count: 2,
            by_rule: BTreeMap::from([("github-token".to_string(), 2u64)]),
            by_field: BTreeMap::from([("process_execs".to_string(), 2u64)]),
            key_scope: "host_local".to_string(),
            key_id: "hmac-sha256:abc123".to_string(),
        }
    }

    #[test]
    fn redaction_receipt_contract_shape_and_roundtrip() {
        let receipt = RedactionReceipt::new(sample_redaction("shape_and_flag"));
        let value = serde_json::to_value(&receipt).unwrap();
        // The schema tag sits alongside the flattened receipt fields (a standalone carrier shape).
        assert_eq!(value["schema"], "assay.redaction_receipt.v0");
        assert_eq!(value["mode"], "shape_and_flag");
        assert_eq!(value["redacted_count"], 2);
        assert_eq!(value["key_id"], "hmac-sha256:abc123");
        let back: RedactionReceipt = serde_json::from_value(value).unwrap();
        assert_eq!(back, receipt);
    }

    #[test]
    fn redaction_receipt_status_is_expectation_aware() {
        assert_eq!(
            RedactionReceipt::new(sample_redaction("shape_and_flag")).status(),
            RedactionReceiptStatus::Active
        );
        assert_eq!(
            RedactionReceipt::new(sample_redaction("disabled_unsafe")).status(),
            RedactionReceiptStatus::Blocked
        );
        assert_eq!(
            RedactionReceipt::new(sample_redaction("future_mode")).status(),
            RedactionReceiptStatus::Unsupported
        );
    }

    #[test]
    fn ringbuf_drops_force_partial_kernel_layer() {
        let health = ObservationHealth::new("run_001", "linux").with_ringbuf_drops(2);

        assert_eq!(health.kernel_layer, KernelLayerStatus::PartialRingbufDrops);
        assert_eq!(health.ringbuf_drops, 2);
        assert_eq!(
            health.network_protocol_coverage,
            NetworkProtocolCoverageStatus::Absent
        );
    }

    #[test]
    fn new_defaults_do_not_claim_observation() {
        let health = ObservationHealth::new("run_001", "linux");

        assert_eq!(health.kernel_layer, KernelLayerStatus::Absent);
        assert_eq!(health.cgroup_correlation, CgroupCorrelationStatus::Partial);
        assert_eq!(
            health.network_endpoint_claim_scope,
            NetworkEndpointClaimScope::NotApplicable
        );
    }

    #[test]
    fn non_linux_forces_absent_kernel_layer() {
        let health = ObservationHealth::new("run_001", "macos");

        assert_eq!(health.kernel_layer, KernelLayerStatus::Absent);
        assert_eq!(
            health.network_protocol_coverage,
            NetworkProtocolCoverageStatus::Absent
        );
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
