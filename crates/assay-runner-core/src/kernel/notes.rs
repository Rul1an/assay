use assay_runner_schema::{
    CgroupCorrelationStatus, NetworkEndpointClaimScope, NetworkProtocolCoverageStatus,
};

use super::stats::{RingbufDropBreakdown, RingbufEmittedBreakdown, TracepointHookBreakdown};

pub(super) struct KernelCaptureNote {
    pub(super) event_count: u64,
    pub(super) ringbuf_drops: u64,
    pub(super) drop_breakdown: RingbufDropBreakdown,
    pub(super) emitted_breakdown: RingbufEmittedBreakdown,
    pub(super) tracepoint_hook_breakdown: TracepointHookBreakdown,
    pub(super) filtered_loader_events: u64,
    pub(super) filtered_loader_top: Vec<(String, u64)>,
    pub(super) cgroup_correlation: CgroupCorrelationStatus,
    pub(super) network_protocol_coverage: NetworkProtocolCoverageStatus,
    pub(super) network_endpoint_claim_scope: NetworkEndpointClaimScope,
}

pub(super) fn kernel_capture_note(input: KernelCaptureNote) -> String {
    let KernelCaptureNote {
        event_count,
        ringbuf_drops,
        drop_breakdown,
        emitted_breakdown,
        tracepoint_hook_breakdown,
        filtered_loader_events,
        filtered_loader_top,
        cgroup_correlation,
        network_protocol_coverage,
        network_endpoint_claim_scope,
    } = input;
    let network_protocol_coverage = network_protocol_coverage_label(network_protocol_coverage);
    let network_endpoint_claim_scope =
        network_endpoint_claim_scope_label(network_endpoint_claim_scope);

    // Surfaced only when a sendto/sendmsg with no recoverable peer address was
    // actually observed, so runs that never hit this path produce a
    // byte-identical note (the load-bearing invariant for existing archives).
    // Socket type is not classified here, so this counts any address-less send
    // (including connected stream sends), not datagram-specifically.
    let send_no_peer =
        tracepoint_hook_breakdown.sendto_no_peer + tracepoint_hook_breakdown.sendmsg_no_peer;
    let no_peer_suffix = if send_no_peer > 0 {
        format!(
            " send_no_recoverable_peer=sendto:{} sendmsg:{}",
            tracepoint_hook_breakdown.sendto_no_peer, tracepoint_hook_breakdown.sendmsg_no_peer
        )
    } else {
        String::new()
    };

    // sendto/sendmsg to a non-IP family (e.g. AF_UNIX). Surfaced only when
    // non-zero, for the same byte-identical-archive invariant. Socket type is not
    // classified; these are not IP peers and never raise the coverage descriptor.
    let send_non_ip = tracepoint_hook_breakdown.sendto_non_ip_family
        + tracepoint_hook_breakdown.sendmsg_non_ip_family;
    let non_ip_suffix = if send_non_ip > 0 {
        format!(
            " send_non_ip_family=sendto:{} sendmsg:{}",
            tracepoint_hook_breakdown.sendto_non_ip_family,
            tracepoint_hook_breakdown.sendmsg_non_ip_family
        )
    } else {
        String::new()
    };

    match cgroup_correlation {
        CgroupCorrelationStatus::Clean if ringbuf_drops == 0 => {
            format!(
                "s2_kernel_capture: monitor_events={event_count} ringbuf_drops={ringbuf_drops} network_protocol_coverage={network_protocol_coverage} network_endpoint_claim_scope={network_endpoint_claim_scope}{no_peer_suffix}{non_ip_suffix}"
            )
        }
        CgroupCorrelationStatus::Clean => {
            let filtered_top = filtered_loader_top
                .into_iter()
                .map(|(path, count)| format!("{count}x:{path}"))
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "s2_kernel_capture: monitor_events={event_count} ringbuf_drops={ringbuf_drops} network_protocol_coverage={network_protocol_coverage} network_endpoint_claim_scope={network_endpoint_claim_scope} drop_breakdown=tracepoint:{} lsm:{} socket:{} emitted=tracepoint:{} lsm:{} socket:{} tracepoint_hooks=openat:{}/{} openat2:{}/{} connect:{}/{} sendto:{}/{} sendmsg:{}/{} filtered_loader_events={filtered_loader_events} filtered_loader_top=[{filtered_top}]{no_peer_suffix}{non_ip_suffix}",
                drop_breakdown.tracepoint,
                drop_breakdown.lsm,
                drop_breakdown.socket,
                emitted_breakdown.tracepoint,
                emitted_breakdown.lsm,
                emitted_breakdown.socket,
                tracepoint_hook_breakdown.openat_emitted,
                tracepoint_hook_breakdown.openat_dropped,
                tracepoint_hook_breakdown.openat2_emitted,
                tracepoint_hook_breakdown.openat2_dropped,
                tracepoint_hook_breakdown.connect_emitted,
                tracepoint_hook_breakdown.connect_dropped,
                tracepoint_hook_breakdown.sendto_emitted,
                tracepoint_hook_breakdown.sendto_dropped,
                tracepoint_hook_breakdown.sendmsg_emitted,
                tracepoint_hook_breakdown.sendmsg_dropped,
            )
        }
        CgroupCorrelationStatus::Partial | CgroupCorrelationStatus::Failed => format!(
            "s2_kernel_capture: monitor_events={event_count} cgroup_correlation={cgroup_correlation:?} kernel_layer downgraded to absent"
        ),
    }
}

fn network_endpoint_claim_scope_label(status: NetworkEndpointClaimScope) -> &'static str {
    match status {
        NetworkEndpointClaimScope::Unknown => "unknown",
        NetworkEndpointClaimScope::NotApplicable => "not_applicable",
        NetworkEndpointClaimScope::DiagnosticOnly => "diagnostic_only",
        NetworkEndpointClaimScope::PeerSet => "peer_set",
    }
}

fn network_protocol_coverage_label(status: NetworkProtocolCoverageStatus) -> &'static str {
    match status {
        NetworkProtocolCoverageStatus::Unknown => "unknown",
        NetworkProtocolCoverageStatus::Absent => "absent",
        NetworkProtocolCoverageStatus::ConnectOnly => "connect_only",
        NetworkProtocolCoverageStatus::DatagramPeerObserved => "datagram_peer_observed",
        NetworkProtocolCoverageStatus::ConnectAndDatagramPeerObserved => {
            "connect_and_datagram_peer_observed"
        }
    }
}
