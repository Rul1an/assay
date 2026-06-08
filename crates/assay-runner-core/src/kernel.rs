use crate::{run::is_safe_run_id, RunnerSpikeArchive};
use assay_common::{MonitorEvent, EVENT_INODE_RESOLVED};
use assay_monitor::MonitorStatsSnapshot;
use assay_runner_schema::{
    CapabilitySurface, CgroupCorrelationStatus, KernelLayerStatus, NetworkEndpointClaimScope,
    NetworkProtocolCoverageStatus,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

mod decode;
mod health;
mod notes;
mod stats;
#[cfg(test)]
mod tests;

use decode::{decode_monitor_event, is_loader_telemetry_path};
use health::{
    health_ringbuf_drops, kernel_layer_for, network_endpoint_claim_scope_for,
    network_protocol_coverage_for,
};
use notes::{kernel_capture_note, KernelCaptureNote};
use stats::{
    ringbuf_drop_breakdown, ringbuf_drop_delta, ringbuf_emitted_breakdown,
    top_filtered_loader_values, tracepoint_hook_breakdown, RingbufDropBreakdown,
    RingbufEmittedBreakdown, TracepointHookBreakdown,
};

pub const KERNEL_EVENT_SCHEMA: &str = "assay.runner.kernel_event.v0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KernelLayerEvent {
    pub schema: String,
    pub run_id: String,
    pub seq: u64,
    pub pid: u32,
    pub event_type: u32,
    pub kind: String,
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolve: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_value: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_mode: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub operation_flags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KernelLayerCapture {
    pub run_id: String,
    pub kernel_layer_ndjson: Vec<u8>,
    pub capability_surface: CapabilitySurface,
    pub event_count: u64,
    pub ringbuf_drops: u64,
    drop_breakdown: RingbufDropBreakdown,
    emitted_breakdown: RingbufEmittedBreakdown,
    tracepoint_hook_breakdown: TracepointHookBreakdown,
    filtered_loader_events: u64,
    filtered_loader_top: Vec<(String, u64)>,
}

#[derive(Debug, Error)]
pub enum KernelLayerError {
    #[error("kernel layer run_id must not be empty")]
    EmptyRunId,
    #[error("kernel layer run_id may only contain ASCII letters, digits, '_' and '-'")]
    UnsafeRunId,
    #[error("kernel layer run_id mismatch: expected {expected}, found {actual}")]
    RunIdMismatch { expected: String, actual: String },
    #[error("invalid capability surface: {0}")]
    CapabilitySurface(#[from] assay_runner_schema::CapabilitySurfaceError),
    #[error("kernel event serialization failed: {0}")]
    Json(#[from] serde_json::Error),
}

pub struct KernelLayerBuilder {
    run_id: String,
    next_seq: u64,
    kernel_layer_ndjson: Vec<u8>,
    capability_surface: CapabilitySurface,
    filtered_loader_events: u64,
    filtered_loader_values: BTreeMap<String, u64>,
}

impl KernelLayerBuilder {
    pub fn new(run_id: impl Into<String>) -> Result<Self, KernelLayerError> {
        let run_id = run_id.into();
        if run_id.is_empty() {
            return Err(KernelLayerError::EmptyRunId);
        }
        if !is_safe_run_id(&run_id) {
            return Err(KernelLayerError::UnsafeRunId);
        }
        Ok(Self {
            capability_surface: CapabilitySurface::new(run_id.clone()),
            run_id,
            next_seq: 0,
            kernel_layer_ndjson: Vec::new(),
            filtered_loader_events: 0,
            filtered_loader_values: BTreeMap::new(),
        })
    }

    pub fn push_monitor_event(&mut self, event: &MonitorEvent) -> Result<(), KernelLayerError> {
        if event.event_type == EVENT_INODE_RESOLVED {
            return Ok(());
        }

        let decoded = decode_monitor_event(event);
        if decoded.kind == "openat"
            && decoded
                .value
                .as_deref()
                .is_some_and(is_loader_telemetry_path)
        {
            if let Some(path) = decoded.value {
                self.filtered_loader_events += 1;
                *self.filtered_loader_values.entry(path).or_insert(0) += 1;
            }
            return Ok(());
        }

        match (decoded.kind.as_str(), decoded.value.as_deref()) {
            ("openat", Some(path)) | ("file_blocked", Some(path)) => {
                self.capability_surface.add_filesystem_path(path);
            }
            ("connect", Some(endpoint))
            | ("connect_blocked", Some(endpoint))
            | ("sendto", Some(endpoint))
            | ("sendmsg", Some(endpoint)) => {
                self.capability_surface.add_network_endpoint(endpoint);
            }
            ("exec", Some(path)) => {
                self.capability_surface.add_process_exec(path);
            }
            _ => {}
        }

        let record = KernelLayerEvent {
            schema: KERNEL_EVENT_SCHEMA.to_string(),
            run_id: self.run_id.clone(),
            seq: self.next_seq,
            pid: event.pid,
            event_type: event.event_type,
            kind: decoded.kind,
            value: decoded.value,
            flags: decoded.flags,
            mode: decoded.mode,
            resolve: decoded.resolve,
            return_value: decoded.return_value,
            access_mode: decoded.access_mode,
            operation_flags: decoded.operation_flags,
            status: decoded.status,
        };
        self.next_seq += 1;
        serde_json::to_writer(&mut self.kernel_layer_ndjson, &record)?;
        self.kernel_layer_ndjson.push(b'\n');
        Ok(())
    }

    pub fn finish(
        self,
        before: &MonitorStatsSnapshot,
        after: &MonitorStatsSnapshot,
    ) -> KernelLayerCapture {
        KernelLayerCapture {
            run_id: self.run_id,
            kernel_layer_ndjson: self.kernel_layer_ndjson,
            capability_surface: self.capability_surface,
            event_count: self.next_seq,
            ringbuf_drops: ringbuf_drop_delta(before, after),
            drop_breakdown: ringbuf_drop_breakdown(before, after),
            emitted_breakdown: ringbuf_emitted_breakdown(before, after),
            tracepoint_hook_breakdown: tracepoint_hook_breakdown(before, after),
            filtered_loader_events: self.filtered_loader_events,
            filtered_loader_top: top_filtered_loader_values(self.filtered_loader_values, 5),
        }
    }
}

impl KernelLayerCapture {
    /// Apply this capture to the archive.
    ///
    /// Replaces any previously-applied kernel layer NDJSON, merges the
    /// capability surface, and updates observation health. The caller supplies
    /// cgroup attribution health; non-clean cgroup correlation downgrades the
    /// kernel layer to absent because scoped kernel attribution is not complete.
    pub fn apply_to_archive(
        self,
        archive: &mut RunnerSpikeArchive,
        cgroup_correlation: CgroupCorrelationStatus,
    ) -> Result<(), KernelLayerError> {
        let KernelLayerCapture {
            run_id,
            kernel_layer_ndjson,
            capability_surface,
            event_count,
            ringbuf_drops,
            drop_breakdown,
            emitted_breakdown,
            tracepoint_hook_breakdown,
            filtered_loader_events,
            filtered_loader_top,
        } = self;
        let mut network_protocol_coverage =
            network_protocol_coverage_for(tracepoint_hook_breakdown);

        if archive.run_id != run_id {
            return Err(KernelLayerError::RunIdMismatch {
                expected: archive.run_id.clone(),
                actual: run_id,
            });
        }

        archive.kernel_layer_ndjson = kernel_layer_ndjson;
        archive.capability_surface.merge_from(&capability_surface)?;
        if archive.observation_health.platform == "linux" {
            archive.observation_health.ringbuf_drops =
                health_ringbuf_drops(ringbuf_drops, cgroup_correlation);
            archive.observation_health.kernel_layer =
                kernel_layer_for(ringbuf_drops, cgroup_correlation);
            archive.observation_health.cgroup_correlation = cgroup_correlation;
            if archive.observation_health.kernel_layer == KernelLayerStatus::Absent {
                archive.observation_health.network_protocol_coverage =
                    NetworkProtocolCoverageStatus::Absent;
                archive.observation_health.network_endpoint_claim_scope =
                    NetworkEndpointClaimScope::NotApplicable;
            } else {
                if archive.observation_health.kernel_layer == KernelLayerStatus::PartialRingbufDrops
                    && network_protocol_coverage == NetworkProtocolCoverageStatus::Absent
                {
                    network_protocol_coverage = NetworkProtocolCoverageStatus::Unknown;
                }
                archive.observation_health.network_protocol_coverage = network_protocol_coverage;
                archive.observation_health.network_endpoint_claim_scope =
                    network_endpoint_claim_scope_for(network_protocol_coverage);
            }
        } else {
            archive.observation_health.ringbuf_drops = 0;
            archive.observation_health.kernel_layer = KernelLayerStatus::Absent;
            archive.observation_health.cgroup_correlation = CgroupCorrelationStatus::Partial;
            archive.observation_health.network_protocol_coverage =
                NetworkProtocolCoverageStatus::Absent;
            archive.observation_health.network_endpoint_claim_scope =
                NetworkEndpointClaimScope::NotApplicable;
        }
        archive.observation_health.notes.retain(|note| {
            !note.starts_with("contract_only_mode:") && !note.starts_with("s2_kernel_capture:")
        });
        archive
            .observation_health
            .notes
            .push(kernel_capture_note(KernelCaptureNote {
                event_count,
                ringbuf_drops,
                drop_breakdown,
                emitted_breakdown,
                tracepoint_hook_breakdown,
                filtered_loader_events,
                filtered_loader_top,
                cgroup_correlation,
                network_protocol_coverage,
                network_endpoint_claim_scope: archive
                    .observation_health
                    .network_endpoint_claim_scope,
            }));
        Ok(())
    }
}
