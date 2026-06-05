use assay_runner_schema::{
    CgroupCorrelationStatus, KernelLayerStatus, NetworkProtocolCoverageStatus,
};

use super::stats::TracepointHookBreakdown;

pub(super) fn network_protocol_coverage_for(
    tracepoints: TracepointHookBreakdown,
) -> NetworkProtocolCoverageStatus {
    let has_connect = tracepoints.connect_emitted > 0;
    let has_datagram_peer = tracepoints.sendto_emitted > 0 || tracepoints.sendmsg_emitted > 0;
    match (has_connect, has_datagram_peer) {
        (true, true) => NetworkProtocolCoverageStatus::ConnectAndDatagramPeerObserved,
        (false, true) => NetworkProtocolCoverageStatus::DatagramPeerObserved,
        _ => NetworkProtocolCoverageStatus::ConnectOnly,
    }
}

pub(super) fn kernel_layer_for(
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

pub(super) fn health_ringbuf_drops(
    ringbuf_drops: u64,
    cgroup_correlation: CgroupCorrelationStatus,
) -> u64 {
    if cgroup_correlation == CgroupCorrelationStatus::Clean {
        ringbuf_drops
    } else {
        0
    }
}
