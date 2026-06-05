use assay_monitor::MonitorStatsSnapshot;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct RingbufDropBreakdown {
    pub(super) tracepoint: u64,
    pub(super) lsm: u64,
    pub(super) socket: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct RingbufEmittedBreakdown {
    pub(super) tracepoint: u64,
    pub(super) lsm: u64,
    pub(super) socket: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct TracepointHookBreakdown {
    pub(super) openat_emitted: u64,
    pub(super) openat_dropped: u64,
    pub(super) openat2_emitted: u64,
    pub(super) openat2_dropped: u64,
    pub(super) connect_emitted: u64,
    pub(super) connect_dropped: u64,
    pub(super) sendto_emitted: u64,
    pub(super) sendto_dropped: u64,
    pub(super) sendmsg_emitted: u64,
    pub(super) sendmsg_dropped: u64,
    pub(super) sendto_no_peer: u64,
    pub(super) sendmsg_no_peer: u64,
    pub(super) sendto_non_ip_family: u64,
    pub(super) sendmsg_non_ip_family: u64,
}

pub(super) fn ringbuf_drop_delta(
    before: &MonitorStatsSnapshot,
    after: &MonitorStatsSnapshot,
) -> u64 {
    after
        .total_ringbuf_dropped()
        .saturating_sub(before.total_ringbuf_dropped())
}

pub(super) fn ringbuf_drop_breakdown(
    before: &MonitorStatsSnapshot,
    after: &MonitorStatsSnapshot,
) -> RingbufDropBreakdown {
    RingbufDropBreakdown {
        tracepoint: u64::from(
            after
                .tracepoint_ringbuf_dropped
                .saturating_sub(before.tracepoint_ringbuf_dropped),
        ),
        lsm: u64::from(
            after
                .lsm_ringbuf_dropped
                .saturating_sub(before.lsm_ringbuf_dropped),
        ),
        socket: after
            .socket_ringbuf_dropped
            .saturating_sub(before.socket_ringbuf_dropped),
    }
}

pub(super) fn ringbuf_emitted_breakdown(
    before: &MonitorStatsSnapshot,
    after: &MonitorStatsSnapshot,
) -> RingbufEmittedBreakdown {
    RingbufEmittedBreakdown {
        tracepoint: u64::from(
            after
                .tracepoint_events_emitted
                .saturating_sub(before.tracepoint_events_emitted),
        ),
        lsm: u64::from(
            after
                .lsm_events_emitted
                .saturating_sub(before.lsm_events_emitted),
        ),
        socket: after
            .socket_events_emitted
            .saturating_sub(before.socket_events_emitted),
    }
}

pub(super) fn tracepoint_hook_breakdown(
    before: &MonitorStatsSnapshot,
    after: &MonitorStatsSnapshot,
) -> TracepointHookBreakdown {
    TracepointHookBreakdown {
        openat_emitted: u64::from(
            after
                .openat_events_emitted
                .saturating_sub(before.openat_events_emitted),
        ),
        openat_dropped: u64::from(
            after
                .openat_ringbuf_dropped
                .saturating_sub(before.openat_ringbuf_dropped),
        ),
        openat2_emitted: u64::from(
            after
                .openat2_events_emitted
                .saturating_sub(before.openat2_events_emitted),
        ),
        openat2_dropped: u64::from(
            after
                .openat2_ringbuf_dropped
                .saturating_sub(before.openat2_ringbuf_dropped),
        ),
        connect_emitted: u64::from(
            after
                .connect_events_emitted
                .saturating_sub(before.connect_events_emitted),
        ),
        connect_dropped: u64::from(
            after
                .connect_ringbuf_dropped
                .saturating_sub(before.connect_ringbuf_dropped),
        ),
        sendto_emitted: u64::from(
            after
                .sendto_events_emitted
                .saturating_sub(before.sendto_events_emitted),
        ),
        sendto_dropped: u64::from(
            after
                .sendto_ringbuf_dropped
                .saturating_sub(before.sendto_ringbuf_dropped),
        ),
        sendmsg_emitted: u64::from(
            after
                .sendmsg_events_emitted
                .saturating_sub(before.sendmsg_events_emitted),
        ),
        sendmsg_dropped: u64::from(
            after
                .sendmsg_ringbuf_dropped
                .saturating_sub(before.sendmsg_ringbuf_dropped),
        ),
        sendto_no_peer: u64::from(after.sendto_no_peer.saturating_sub(before.sendto_no_peer)),
        sendmsg_no_peer: u64::from(after.sendmsg_no_peer.saturating_sub(before.sendmsg_no_peer)),
        sendto_non_ip_family: u64::from(
            after
                .sendto_non_ip_family
                .saturating_sub(before.sendto_non_ip_family),
        ),
        sendmsg_non_ip_family: u64::from(
            after
                .sendmsg_non_ip_family
                .saturating_sub(before.sendmsg_non_ip_family),
        ),
    }
}

pub(super) fn top_filtered_loader_values(
    values: BTreeMap<String, u64>,
    limit: usize,
) -> Vec<(String, u64)> {
    let mut values: Vec<_> = values.into_iter().collect();
    values.sort_by(|(left_path, left_count), (right_path, right_count)| {
        right_count
            .cmp(left_count)
            .then_with(|| left_path.cmp(right_path))
    });
    values.truncate(limit);
    values
}
