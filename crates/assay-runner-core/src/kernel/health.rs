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
    event_size_mismatch: u64,
    cgroup_correlation: CgroupCorrelationStatus,
) -> KernelLayerStatus {
    // Both ringbuf drops and event-size mismatches are lost events: records that never reached
    // the decoded stream. Either one makes the kernel layer incomplete. The counts stay separate
    // on the capture for diagnosis; here we only ask "was anything lost?", so summing for the
    // 0-vs-nonzero decision is not a reporting conflation.
    let lost = ringbuf_drops.saturating_add(event_size_mismatch);
    match (lost, cgroup_correlation) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_run_with_no_loss_is_complete() {
        assert_eq!(
            kernel_layer_for(0, 0, CgroupCorrelationStatus::Clean),
            KernelLayerStatus::Complete
        );
    }

    #[test]
    fn event_size_mismatch_alone_degrades_kernel_layer() {
        // A stale-object size mismatch is lost events, so even with zero ringbuf drops the kernel
        // layer must not read as complete. "Saw nothing" is not clean if records were dropped.
        assert_eq!(
            kernel_layer_for(0, 3, CgroupCorrelationStatus::Clean),
            KernelLayerStatus::PartialRingbufDrops
        );
    }

    #[test]
    fn ringbuf_drops_alone_still_degrades() {
        assert_eq!(
            kernel_layer_for(2, 0, CgroupCorrelationStatus::Clean),
            KernelLayerStatus::PartialRingbufDrops
        );
    }

    #[test]
    fn non_clean_cgroup_is_absent_regardless_of_loss() {
        assert_eq!(
            kernel_layer_for(0, 0, CgroupCorrelationStatus::Partial),
            KernelLayerStatus::Absent
        );
        assert_eq!(
            kernel_layer_for(0, 9, CgroupCorrelationStatus::Failed),
            KernelLayerStatus::Absent
        );
    }
}
