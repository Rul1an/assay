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

const TEST_MAX_RECORDS: u64 = 10_000;
const TEST_MAX_NDJSON_BYTES: usize = 4 * 1024 * 1024;
const TEST_MAX_FILTERED_LOADER_PATHS: usize = 1024;
const GOLDEN_EXEC_TRUE_EMPTY_RUN_ID_LINE: &str =
    "{\"schema\":\"assay.runner.kernel_event.v0\",\"run_id\":\"\",\"seq\":0,\"pid\":42,\"event_type\":4,\"kind\":\"exec\",\"value\":\"/usr/bin/true\"}\n";

fn four_event_byte_cap_run_id() -> String {
    let line_len = TEST_MAX_NDJSON_BYTES / 4;
    "r".repeat(line_len - GOLDEN_EXEC_TRUE_EMPTY_RUN_ID_LINE.len())
}

fn sentinel_archive(run_id: &str) -> RunnerSpikeArchive {
    let mut archive = RunnerSpikeArchive::empty(run_id, "linux");
    archive.events_ndjson = b"{\"sentinel\":\"events\"}\n".to_vec();
    archive.kernel_layer_ndjson = b"{\"sentinel\":\"kernel\"}\n".to_vec();
    archive.policy_layer_ndjson = b"{\"sentinel\":\"policy\"}\n".to_vec();
    archive.sdk_layer_ndjson = b"{\"sentinel\":\"sdk\"}\n".to_vec();
    archive
        .capability_surface
        .add_filesystem_path("/sentinel/existing");
    archive
        .observation_health
        .notes
        .push("s2_kernel_capture: sentinel".to_string());
    archive
}

mod archive_health;
mod events;
mod network_health;
