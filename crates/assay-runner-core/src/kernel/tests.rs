use super::*;
use assay_common::{
    MonitorEvent, EVENT_CONNECT, EVENT_CONNECT_BLOCKED, EVENT_EXEC, EVENT_FILE_BLOCKED,
    EVENT_INODE_RESOLVED, EVENT_OPENAT, EVENT_SENDMSG, EVENT_SENDTO,
};
use assay_monitor::MonitorStatsSnapshot;

fn event(event_type: u32, value: &[u8]) -> MonitorEvent {
    let mut event = MonitorEvent::zeroed();
    event.pid = 42;
    event.event_type = event_type;
    event.data[..value.len()].copy_from_slice(value);
    event
}

fn open_event(value: &[u8], flags: u64, return_value: i64) -> MonitorEvent {
    let mut event = event(EVENT_OPENAT, value);
    event.flags = flags;
    event.mode = 0o644;
    event.return_value = return_value;
    event
}

mod archive_health;
mod events;
mod network_health;
