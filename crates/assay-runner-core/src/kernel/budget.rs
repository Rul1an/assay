use std::io::Write;
use thiserror::Error;

use super::KernelLayerEvent;

pub const MAX_KERNEL_LAYER_EVENTS: u64 = 10_000;
pub const MAX_KERNEL_LAYER_NDJSON_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_FILTERED_LOADER_PATHS: usize = 1024;

const INITIAL_SCRATCH_CAPACITY: usize = 256;

/// Typed resource-budget refusal latched on the first capture ceiling crossing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum KernelCaptureRefusal {
    #[error("kernel capture exceeded retained event budget of {limit} events")]
    EventLimitExceeded { limit: u64 },
    #[error("kernel capture exceeded NDJSON byte budget of {limit} bytes")]
    NdjsonByteLimitExceeded { limit: usize },
    #[error("kernel capture exceeded filtered loader path budget of {limit} distinct paths")]
    FilteredLoaderPathLimitExceeded { limit: usize },
}

pub(super) struct BoundedScratchWriter {
    bytes: Vec<u8>,
    max_bytes: usize,
    overflowed: bool,
}

impl BoundedScratchWriter {
    pub(super) fn new(max_bytes: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(max_bytes.min(INITIAL_SCRATCH_CAPACITY)),
            max_bytes,
            overflowed: false,
        }
    }

    pub(super) fn overflowed(&self) -> bool {
        self.overflowed
    }

    pub(super) fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

impl Write for BoundedScratchWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let remaining = self.max_bytes.saturating_sub(self.bytes.len());
        if buf.len() > remaining {
            self.overflowed = true;
            return Err(std::io::Error::new(
                std::io::ErrorKind::WriteZero,
                "kernel event NDJSON byte budget exceeded",
            ));
        }
        self.bytes.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub(super) fn serialize_bounded_ndjson_line(
    record: &KernelLayerEvent,
    max_bytes: usize,
) -> Result<Option<Vec<u8>>, serde_json::Error> {
    let mut scratch = BoundedScratchWriter::new(max_bytes);
    if let Err(err) = serde_json::to_writer(&mut scratch, record) {
        if scratch.overflowed() {
            return Ok(None);
        }
        return Err(err);
    }
    if scratch.write_all(b"\n").is_err() {
        return Ok(None);
    }
    Ok(Some(scratch.into_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::{KernelLayerBuilder, KernelLayerError, RunnerSpikeArchive};
    use assay_common::{MonitorEvent, EVENT_OPENAT};
    use assay_monitor::MonitorStatsSnapshot;
    use assay_runner_schema::CgroupCorrelationStatus;

    fn openat_event(path: &[u8]) -> MonitorEvent {
        let mut ev = MonitorEvent::zeroed();
        ev.pid = 42;
        ev.event_type = EVENT_OPENAT;
        ev.data[..path.len()].copy_from_slice(path);
        ev
    }

    #[test]
    fn refusal_accessor_reports_typed_reason_and_apply_propagates_it() {
        let mut clean_builder = KernelLayerBuilder::new("run_001").unwrap();
        clean_builder
            .push_monitor_event(&openat_event(b"/tmp/ok\0"))
            .unwrap();
        let clean = clean_builder.finish(
            &MonitorStatsSnapshot::default(),
            &MonitorStatsSnapshot::default(),
        );
        assert_eq!(clean.refusal(), None);

        let mut refused_builder =
            KernelLayerBuilder::new("r".repeat(MAX_KERNEL_LAYER_NDJSON_BYTES)).unwrap();
        refused_builder
            .push_monitor_event(&openat_event(b"/tmp/refused\0"))
            .unwrap();
        let refused = refused_builder.finish(
            &MonitorStatsSnapshot::default(),
            &MonitorStatsSnapshot::default(),
        );
        assert_eq!(
            refused.refusal(),
            Some(KernelCaptureRefusal::NdjsonByteLimitExceeded {
                limit: MAX_KERNEL_LAYER_NDJSON_BYTES,
            })
        );

        let mut archive = RunnerSpikeArchive::empty(&refused.run_id, "linux");
        let err = refused
            .apply_to_archive(&mut archive, CgroupCorrelationStatus::Clean)
            .unwrap_err();
        assert!(matches!(
            err,
            KernelLayerError::CaptureRefused(KernelCaptureRefusal::NdjsonByteLimitExceeded {
                limit: MAX_KERNEL_LAYER_NDJSON_BYTES,
            })
        ));

        let mut event_cap_builder = KernelLayerBuilder::new("run_001").unwrap();
        event_cap_builder.next_seq = MAX_KERNEL_LAYER_EVENTS;
        event_cap_builder
            .push_monitor_event(&openat_event(b"/tmp/over-events\0"))
            .unwrap();
        let event_cap = event_cap_builder.finish(
            &MonitorStatsSnapshot::default(),
            &MonitorStatsSnapshot::default(),
        );
        assert_eq!(
            event_cap.refusal(),
            Some(KernelCaptureRefusal::EventLimitExceeded {
                limit: MAX_KERNEL_LAYER_EVENTS,
            })
        );
    }
}
