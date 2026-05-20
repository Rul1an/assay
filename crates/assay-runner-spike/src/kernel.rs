use crate::{CapabilitySurface, CgroupCorrelationStatus, KernelLayerStatus, RunnerSpikeArchive};
use assay_common::{
    MonitorEvent, EVENT_CONNECT, EVENT_CONNECT_BLOCKED, EVENT_EXEC, EVENT_FILE_BLOCKED,
    EVENT_OPENAT,
};
use assay_monitor::MonitorStatsSnapshot;
use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, Ipv6Addr};
use thiserror::Error;

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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KernelLayerCapture {
    pub run_id: String,
    pub kernel_layer_ndjson: Vec<u8>,
    pub capability_surface: CapabilitySurface,
    pub event_count: u64,
    pub ringbuf_drops: u64,
}

#[derive(Debug, Error)]
pub enum KernelLayerError {
    #[error("kernel layer run_id must not be empty")]
    EmptyRunId,
    #[error("kernel layer run_id mismatch: expected {expected}, found {actual}")]
    RunIdMismatch { expected: String, actual: String },
    #[error("kernel event serialization failed: {0}")]
    Json(#[from] serde_json::Error),
}

pub struct KernelLayerBuilder {
    run_id: String,
    next_seq: u64,
    kernel_layer_ndjson: Vec<u8>,
    capability_surface: CapabilitySurface,
}

impl KernelLayerBuilder {
    pub fn new(run_id: impl Into<String>) -> Result<Self, KernelLayerError> {
        let run_id = run_id.into();
        if run_id.is_empty() {
            return Err(KernelLayerError::EmptyRunId);
        }
        Ok(Self {
            capability_surface: CapabilitySurface::new(run_id.clone()),
            run_id,
            next_seq: 0,
            kernel_layer_ndjson: Vec::new(),
        })
    }

    pub fn push_monitor_event(&mut self, event: &MonitorEvent) -> Result<(), KernelLayerError> {
        let decoded = decode_monitor_event(event);
        match (decoded.kind.as_str(), decoded.value.as_deref()) {
            ("openat", Some(path)) | ("file_blocked", Some(path)) => {
                self.capability_surface.add_filesystem_prefix(path);
            }
            ("connect", Some(endpoint)) | ("connect_blocked", Some(endpoint)) => {
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
        &self,
        archive: &mut RunnerSpikeArchive,
        cgroup_correlation: CgroupCorrelationStatus,
    ) -> Result<(), KernelLayerError> {
        if archive.run_id != self.run_id {
            return Err(KernelLayerError::RunIdMismatch {
                expected: archive.run_id.clone(),
                actual: self.run_id.clone(),
            });
        }

        archive.kernel_layer_ndjson = self.kernel_layer_ndjson.clone();
        archive
            .capability_surface
            .merge_from(&self.capability_surface);
        archive.observation_health.ringbuf_drops = self.ringbuf_drops;
        if archive.observation_health.platform == "linux" {
            archive.observation_health.kernel_layer =
                kernel_layer_for(self.ringbuf_drops, cgroup_correlation);
            archive.observation_health.cgroup_correlation = cgroup_correlation;
        } else {
            archive.observation_health.kernel_layer = KernelLayerStatus::Absent;
            archive.observation_health.cgroup_correlation = CgroupCorrelationStatus::Partial;
        }
        archive.observation_health.notes.retain(|note| {
            !note.starts_with("contract_only_mode:") && !note.starts_with("s2_kernel_capture:")
        });
        archive.observation_health.notes.push(kernel_capture_note(
            self.event_count,
            self.ringbuf_drops,
            cgroup_correlation,
        ));
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DecodedKernelEvent {
    kind: String,
    value: Option<String>,
}

fn decode_monitor_event(event: &MonitorEvent) -> DecodedKernelEvent {
    match event.event_type {
        EVENT_OPENAT => DecodedKernelEvent {
            kind: "openat".to_string(),
            value: decode_c_string(&event.data),
        },
        EVENT_CONNECT => DecodedKernelEvent {
            kind: "connect".to_string(),
            value: decode_sockaddr_endpoint(&event.data),
        },
        EVENT_EXEC => DecodedKernelEvent {
            kind: "exec".to_string(),
            value: decode_c_string(&event.data),
        },
        EVENT_FILE_BLOCKED => DecodedKernelEvent {
            kind: "file_blocked".to_string(),
            value: decode_c_string(&event.data),
        },
        EVENT_CONNECT_BLOCKED => DecodedKernelEvent {
            kind: "connect_blocked".to_string(),
            value: decode_sockaddr_endpoint(&event.data),
        },
        other => DecodedKernelEvent {
            kind: format!("event_{other}"),
            value: None,
        },
    }
}

fn decode_c_string(bytes: &[u8]) -> Option<String> {
    let end = bytes
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(bytes.len());
    if end == 0 {
        return None;
    }
    Some(String::from_utf8_lossy(&bytes[..end]).to_string())
}

fn decode_sockaddr_endpoint(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 2 {
        return None;
    }
    let family = u16::from_ne_bytes(bytes[0..2].try_into().ok()?);
    match family {
        2 if bytes.len() >= 8 => {
            // AF_INET on Linux; monitor events are emitted by Linux eBPF code.
            let port = u16::from_be_bytes(bytes[2..4].try_into().ok()?);
            let addr = Ipv4Addr::new(bytes[4], bytes[5], bytes[6], bytes[7]);
            Some(format!("{addr}:{port}"))
        }
        10 if bytes.len() >= 28 => {
            // AF_INET6 on Linux; monitor events are emitted by Linux eBPF code.
            let port = u16::from_be_bytes(bytes[2..4].try_into().ok()?);
            let addr = Ipv6Addr::from(<[u8; 16]>::try_from(&bytes[8..24]).ok()?);
            Some(format!("[{addr}]:{port}"))
        }
        _ => None,
    }
}

fn ringbuf_drop_delta(before: &MonitorStatsSnapshot, after: &MonitorStatsSnapshot) -> u64 {
    after
        .total_ringbuf_dropped()
        .saturating_sub(before.total_ringbuf_dropped())
}

fn kernel_layer_for(
    ringbuf_drops: u64,
    cgroup_correlation: CgroupCorrelationStatus,
) -> KernelLayerStatus {
    match (ringbuf_drops, cgroup_correlation) {
        (_, CgroupCorrelationStatus::Failed | CgroupCorrelationStatus::Partial) => {
            KernelLayerStatus::Absent
        }
        (0, CgroupCorrelationStatus::Clean) => KernelLayerStatus::Complete,
        (_, CgroupCorrelationStatus::Clean) => KernelLayerStatus::PartialRingbufDrops,
    }
}

fn kernel_capture_note(
    event_count: u64,
    ringbuf_drops: u64,
    cgroup_correlation: CgroupCorrelationStatus,
) -> String {
    match cgroup_correlation {
        CgroupCorrelationStatus::Clean => {
            format!("s2_kernel_capture: monitor_events={event_count} ringbuf_drops={ringbuf_drops}")
        }
        CgroupCorrelationStatus::Partial | CgroupCorrelationStatus::Failed => format!(
            "s2_kernel_capture: monitor_events={event_count} cgroup_correlation={cgroup_correlation:?} kernel_layer downgraded to absent"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(event_type: u32, value: &[u8]) -> MonitorEvent {
        let mut event = MonitorEvent::zeroed();
        event.pid = 42;
        event.event_type = event_type;
        event.data[..value.len()].copy_from_slice(value);
        event
    }

    #[test]
    fn openat_event_records_filesystem_capability() {
        let mut builder = KernelLayerBuilder::new("run_001").unwrap();

        builder
            .push_monitor_event(&event(EVENT_OPENAT, b"/tmp/assay-known-file\0"))
            .unwrap();
        let capture = builder.finish(
            &MonitorStatsSnapshot::default(),
            &MonitorStatsSnapshot::default(),
        );

        assert!(String::from_utf8(capture.kernel_layer_ndjson.clone())
            .unwrap()
            .contains("\"kind\":\"openat\""));
        assert!(capture
            .capability_surface
            .filesystem_prefixes
            .contains("/tmp/assay-known-file"));
        assert_eq!(capture.ringbuf_drops, 0);
    }

    #[test]
    fn exec_event_records_process_capability() {
        let mut builder = KernelLayerBuilder::new("run_001").unwrap();

        builder
            .push_monitor_event(&event(EVENT_EXEC, b"/usr/bin/true\0"))
            .unwrap();
        let capture = builder.finish(
            &MonitorStatsSnapshot::default(),
            &MonitorStatsSnapshot::default(),
        );

        assert!(capture
            .capability_surface
            .process_execs
            .contains("/usr/bin/true"));
    }

    #[test]
    fn file_blocked_event_records_filesystem_capability() {
        let mut builder = KernelLayerBuilder::new("run_001").unwrap();

        builder
            .push_monitor_event(&event(EVENT_FILE_BLOCKED, b"/etc/passwd\0"))
            .unwrap();
        let capture = builder.finish(
            &MonitorStatsSnapshot::default(),
            &MonitorStatsSnapshot::default(),
        );

        assert!(capture
            .capability_surface
            .filesystem_prefixes
            .contains("/etc/passwd"));
    }

    #[test]
    fn ipv4_connect_event_records_network_capability() {
        let mut sockaddr = [0_u8; 16];
        sockaddr[0..2].copy_from_slice(&2_u16.to_ne_bytes());
        sockaddr[2..4].copy_from_slice(&8080_u16.to_be_bytes());
        sockaddr[4..8].copy_from_slice(&[127, 0, 0, 1]);
        let mut builder = KernelLayerBuilder::new("run_001").unwrap();

        builder
            .push_monitor_event(&event(EVENT_CONNECT, &sockaddr))
            .unwrap();
        let capture = builder.finish(
            &MonitorStatsSnapshot::default(),
            &MonitorStatsSnapshot::default(),
        );

        assert!(capture
            .capability_surface
            .network_endpoints
            .contains("127.0.0.1:8080"));
    }

    #[test]
    fn connect_blocked_event_records_network_capability() {
        let mut sockaddr = [0_u8; 16];
        sockaddr[0..2].copy_from_slice(&2_u16.to_ne_bytes());
        sockaddr[2..4].copy_from_slice(&443_u16.to_be_bytes());
        sockaddr[4..8].copy_from_slice(&[10, 0, 0, 5]);
        let mut builder = KernelLayerBuilder::new("run_001").unwrap();

        builder
            .push_monitor_event(&event(EVENT_CONNECT_BLOCKED, &sockaddr))
            .unwrap();
        let capture = builder.finish(
            &MonitorStatsSnapshot::default(),
            &MonitorStatsSnapshot::default(),
        );

        assert!(capture
            .capability_surface
            .network_endpoints
            .contains("10.0.0.5:443"));
    }

    #[test]
    fn ringbuf_drop_delta_marks_partial_health_when_applied() {
        let before = MonitorStatsSnapshot {
            tracepoint_ringbuf_dropped: 2,
            ..Default::default()
        };
        let after = MonitorStatsSnapshot {
            tracepoint_ringbuf_dropped: 5,
            lsm_ringbuf_dropped: 1,
            ..Default::default()
        };
        let builder = KernelLayerBuilder::new("run_001").unwrap();
        let capture = builder.finish(&before, &after);
        let mut archive = RunnerSpikeArchive::empty("run_001", "linux");

        capture
            .apply_to_archive(&mut archive, CgroupCorrelationStatus::Clean)
            .unwrap();

        assert_eq!(capture.ringbuf_drops, 4);
        assert_eq!(
            archive.observation_health.kernel_layer,
            KernelLayerStatus::PartialRingbufDrops
        );
        assert_eq!(archive.observation_health.ringbuf_drops, 4);
    }

    #[test]
    fn clean_capture_can_mark_kernel_complete() {
        let mut builder = KernelLayerBuilder::new("run_001").unwrap();
        builder
            .push_monitor_event(&event(EVENT_OPENAT, b"/tmp/known\0"))
            .unwrap();
        let capture = builder.finish(
            &MonitorStatsSnapshot::default(),
            &MonitorStatsSnapshot::default(),
        );
        let mut archive = RunnerSpikeArchive::empty("run_001", "linux");

        capture
            .apply_to_archive(&mut archive, CgroupCorrelationStatus::Clean)
            .unwrap();

        assert_eq!(
            archive.observation_health.kernel_layer,
            KernelLayerStatus::Complete
        );
        assert_eq!(
            archive.observation_health.cgroup_correlation,
            CgroupCorrelationStatus::Clean
        );
        assert!(archive
            .observation_health
            .notes
            .iter()
            .any(|note| note.starts_with("s2_kernel_capture:")));
    }

    #[test]
    fn partial_cgroup_correlation_downgrades_kernel_layer_to_absent() {
        let capture = KernelLayerBuilder::new("run_001").unwrap().finish(
            &MonitorStatsSnapshot::default(),
            &MonitorStatsSnapshot::default(),
        );
        let mut archive = RunnerSpikeArchive::empty("run_001", "linux");

        capture
            .apply_to_archive(&mut archive, CgroupCorrelationStatus::Partial)
            .unwrap();

        assert_eq!(
            archive.observation_health.kernel_layer,
            KernelLayerStatus::Absent
        );
        assert_eq!(
            archive.observation_health.cgroup_correlation,
            CgroupCorrelationStatus::Partial
        );
        assert!(archive
            .observation_health
            .notes
            .iter()
            .any(|note| note.contains("kernel_layer downgraded to absent")));
    }

    #[test]
    fn invalid_sockaddr_is_preserved_as_event_without_capability() {
        let mut builder = KernelLayerBuilder::new("run_001").unwrap();

        builder
            .push_monitor_event(&event(EVENT_CONNECT, &[0, 0]))
            .unwrap();
        let capture = builder.finish(
            &MonitorStatsSnapshot::default(),
            &MonitorStatsSnapshot::default(),
        );

        assert!(capture.capability_surface.network_endpoints.is_empty());
        assert!(String::from_utf8(capture.kernel_layer_ndjson)
            .unwrap()
            .contains("\"value\":null"));
    }

    #[test]
    fn apply_rejects_run_id_mismatch() {
        let capture = KernelLayerBuilder::new("run_001").unwrap().finish(
            &MonitorStatsSnapshot::default(),
            &MonitorStatsSnapshot::default(),
        );
        let mut archive = RunnerSpikeArchive::empty("run_002", "linux");

        let err = capture
            .apply_to_archive(&mut archive, CgroupCorrelationStatus::Clean)
            .unwrap_err();

        assert!(matches!(err, KernelLayerError::RunIdMismatch { .. }));
    }

    #[test]
    fn apply_on_non_linux_keeps_kernel_absent() {
        let capture = KernelLayerBuilder::new("run_001").unwrap().finish(
            &MonitorStatsSnapshot::default(),
            &MonitorStatsSnapshot::default(),
        );
        let mut archive = RunnerSpikeArchive::empty("run_001", "macos");

        capture
            .apply_to_archive(&mut archive, CgroupCorrelationStatus::Clean)
            .unwrap();

        assert_eq!(
            archive.observation_health.kernel_layer,
            KernelLayerStatus::Absent
        );
        assert_eq!(
            archive.observation_health.cgroup_correlation,
            CgroupCorrelationStatus::Partial
        );
    }
}
