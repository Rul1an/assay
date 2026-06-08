use assay_runner_schema::{
    CgroupCorrelationStatus, KernelLayerStatus, NetworkEndpointClaimScope,
    NetworkProtocolCoverageStatus,
};

use super::stats::TracepointHookBreakdown;

pub(super) fn network_protocol_coverage_for(
    tracepoints: TracepointHookBreakdown,
) -> NetworkProtocolCoverageStatus {
    let has_connect = tracepoints.connect_emitted > 0;
    let has_datagram_peer = tracepoints.sendto_emitted > 0 || tracepoints.sendmsg_emitted > 0;
    let has_network_hook_drop = tracepoints.connect_dropped > 0
        || tracepoints.sendto_dropped > 0
        || tracepoints.sendmsg_dropped > 0;
    if !has_connect && !has_datagram_peer && has_network_hook_drop {
        return NetworkProtocolCoverageStatus::Unknown;
    }
    match (has_connect, has_datagram_peer) {
        (true, true) => NetworkProtocolCoverageStatus::ConnectAndDatagramPeerObserved,
        (true, false) => NetworkProtocolCoverageStatus::ConnectOnly,
        (false, true) => NetworkProtocolCoverageStatus::DatagramPeerObserved,
        (false, false) => NetworkProtocolCoverageStatus::Absent,
    }
}

pub(super) fn network_endpoint_claim_scope_for(
    network_protocol_coverage: NetworkProtocolCoverageStatus,
) -> NetworkEndpointClaimScope {
    match network_protocol_coverage {
        NetworkProtocolCoverageStatus::Unknown => NetworkEndpointClaimScope::Unknown,
        NetworkProtocolCoverageStatus::Absent => NetworkEndpointClaimScope::NotApplicable,
        NetworkProtocolCoverageStatus::ConnectOnly
        | NetworkProtocolCoverageStatus::DatagramPeerObserved
        | NetworkProtocolCoverageStatus::ConnectAndDatagramPeerObserved => {
            NetworkEndpointClaimScope::DiagnosticOnly
        }
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
