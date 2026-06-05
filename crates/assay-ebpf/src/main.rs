#![no_std]
#![no_main]

#[no_mangle]
#[link_section = "license"]
pub static _LICENSE: [u8; 4] = *b"GPL\0";

mod connect_events;
mod fork_events;
pub mod lsm;
mod open_events;
mod path_filter;
pub mod socket_lsm;
#[allow(dead_code)]
#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
#[allow(non_upper_case_globals)]
#[allow(clippy::all)]
pub mod vmlinux;

use assay_common::{
    MonitorEvent, EVENT_CONNECT, EVENT_OPENAT, EVENT_SENDMSG, EVENT_SENDTO, KEY_DEDUP_OPEN_PATHS,
    KEY_MONITOR_ALL, MONITOR_STATS_LEN, MONITOR_STAT_CONNECT_EVENTS_EMITTED,
    MONITOR_STAT_CONNECT_RINGBUF_DROPPED, MONITOR_STAT_OPENAT2_EVENTS_EMITTED,
    MONITOR_STAT_OPENAT2_RINGBUF_DROPPED, MONITOR_STAT_OPENAT_EVENTS_EMITTED,
    MONITOR_STAT_OPENAT_RINGBUF_DROPPED, MONITOR_STAT_SENDMSG_EVENTS_EMITTED,
    MONITOR_STAT_SENDMSG_NON_IP_FAMILY, MONITOR_STAT_SENDMSG_NO_PEER,
    MONITOR_STAT_SENDMSG_RINGBUF_DROPPED, MONITOR_STAT_SENDTO_EVENTS_EMITTED,
    MONITOR_STAT_SENDTO_NON_IP_FAMILY, MONITOR_STAT_SENDTO_NO_PEER,
    MONITOR_STAT_SENDTO_RINGBUF_DROPPED, MONITOR_STAT_TRACEPOINT_EVENTS_EMITTED,
    MONITOR_STAT_TRACEPOINT_RINGBUF_DROPPED,
};
use aya_ebpf::{
    helpers::{
        bpf_get_current_ancestor_cgroup_id, bpf_get_current_cgroup_id, bpf_get_current_pid_tgid,
    },
    macros::{map, tracepoint},
    maps::{Array, HashMap, PerCpuArray, RingBuf},
    programs::TracePointContext,
};

#[inline(always)]
fn current_tgid() -> u32 {
    (bpf_get_current_pid_tgid() >> 32) as u32
}

#[inline(always)]
pub(crate) fn inc_stat(index: u32) {
    if let Some(val) = STATS.get_ptr_mut(index) {
        unsafe { *val += 1 };
    }
}

// Tracepoint openat can burst during dynamic linker, locale, and subprocess
// startup before runner-spike filters loader telemetry in userspace. Keep
// enough headroom to preserve zero-drop attribution semantics across delegated
// multi-process runs.
#[map]
static EVENTS: RingBuf = RingBuf::with_byte_size(32 * 1024 * 1024, 0);

#[map]
pub static TP_HIT: Array<u64> = Array::with_max_entries(1, 0);

#[map]
pub static MONITORED_PIDS: HashMap<u32, u8> = HashMap::with_max_entries(1024, 0);

/// Map of Cgroup IDs (inodes) we're monitoring.
/// Key: Cgroup ID (u64), Value: 1 (present)
#[map]
pub static MONITORED_CGROUPS: HashMap<u64, u8> = HashMap::with_max_entries(1024, 0);

#[map]
pub static OPEN_PATH_SEEN: HashMap<u64, u8> = HashMap::with_max_entries(4096, 0);

/// Configuration Map for dynamic offsets.
/// Key 0: openat filename offset (default 24)
/// Key 1: connect sockaddr offset (default 24)
#[map]
pub static CONFIG: HashMap<u32, u32> = HashMap::with_max_entries(16, 0);

#[map]
pub static LSM_HIT: Array<u64> = Array::with_max_entries(1, 0);

#[map]
pub static LSM_DENY: Array<u64> = Array::with_max_entries(1, 0);

#[map]
pub static LSM_BYPASS: Array<u64> = Array::with_max_entries(1, 0);

#[map]
pub static DENY_INO: HashMap<assay_common::InodeKeyMap, u32> = HashMap::with_max_entries(1024, 0);

#[map]
pub static LSM_EVENTS: RingBuf = RingBuf::with_byte_size(256 * 1024, 0);

#[map]
pub static STATS: Array<u32> = Array::with_max_entries(MONITOR_STATS_LEN, 0);

#[repr(C)]
#[derive(Clone, Copy)]
struct PendingOpen {
    data: [u8; DATA_LEN],
    flags: u64,
    mode: u64,
    resolve: u64,
}

#[map]
static PENDING_OPEN: HashMap<u64, PendingOpen> = HashMap::with_max_entries(4096, 0);

#[map]
static OPEN_SCRATCH: PerCpuArray<PendingOpen> = PerCpuArray::with_max_entries(1, 0);

const KEY_OFFSET_FILENAME: u32 = 0;
const KEY_OFFSET_SOCKADDR: u32 = 1;
const KEY_OFFSET_FORK_PARENT: u32 = 2;
const KEY_OFFSET_FORK_CHILD: u32 = 3;
const KEY_OFFSET_FILENAME_OPENAT2: u32 = 4;
const KEY_OFFSET_OPENAT_FLAGS: u32 = 5;
const KEY_OFFSET_OPENAT_MODE: u32 = 6;
const KEY_OFFSET_OPENAT2_HOW: u32 = 7;
const KEY_OFFSET_SYSCALL_EXIT_RET: u32 = 8;
const KEY_OFFSET_SENDTO_SOCKADDR: u32 = 11;
const KEY_OFFSET_SENDMSG_MSGHDR: u32 = 12;
const DEFAULT_OFFSET: u32 = 24;

const KEY_MAX_ANCESTOR_DEPTH: u32 = 10;
const MAX_ANCESTOR_DEPTH_HARD: usize = 16;

#[inline(always)]
fn max_ancestor_depth() -> usize {
    let v = unsafe { CONFIG.get(&KEY_MAX_ANCESTOR_DEPTH) }
        .copied()
        .unwrap_or(8);
    let v = v as usize;
    if v > MAX_ANCESTOR_DEPTH_HARD {
        MAX_ANCESTOR_DEPTH_HARD
    } else {
        v
    }
}

#[inline(always)]
fn is_monitored() -> bool {
    // 0. Global Monitor-All Override
    if unsafe { CONFIG.get(&KEY_MONITOR_ALL) }
        .copied()
        .unwrap_or(0)
        != 0
    {
        return true;
    }

    // 1. Check PID (Legacy/Override)
    if unsafe { MONITORED_PIDS.get(&current_tgid()) }.is_some() {
        return true;
    }

    // 2. Check current cgroup
    let current_id = unsafe { bpf_get_current_cgroup_id() };
    if unsafe { MONITORED_CGROUPS.get(&current_id) }.is_some() {
        return true;
    }

    // 3. Scan Ancestors (prevent nested cgroup escape)
    let depth = max_ancestor_depth();
    for i in 0..MAX_ANCESTOR_DEPTH_HARD {
        if i >= depth {
            break;
        }

        let ancestor_id = unsafe { bpf_get_current_ancestor_cgroup_id(i as i32) };
        if ancestor_id == 0 {
            break;
        } // Root or error

        if unsafe { MONITORED_CGROUPS.get(&ancestor_id) }.is_some() {
            return true;
        }
    }

    false
}

const DATA_LEN: usize = 512;

#[inline(always)]
unsafe fn write_event_header(ev: *mut MonitorEvent, pid: u32, event_type: u32) {
    (*ev).pid = pid;
    (*ev).event_type = event_type;
    (*ev).flags = 0;
    (*ev).mode = 0;
    (*ev).resolve = 0;
    (*ev).return_value = 0;
    // Zero payload in-place
    core::ptr::write_bytes((*ev).data.as_mut_ptr(), 0, (*ev).data.len());
}

#[tracepoint]
pub fn assay_monitor_openat(ctx: TracePointContext) -> u32 {
    match open_events::try_openat(ctx) {
        Ok(v) => v,
        Err(v) => v,
    }
}

/// SOTA Coverage: Also monitor openat2 (modern Linux)
#[tracepoint]
pub fn assay_monitor_openat2(ctx: TracePointContext) -> u32 {
    match open_events::try_openat2(ctx) {
        Ok(v) => v,
        Err(v) => v,
    }
}

#[tracepoint]
pub fn assay_monitor_openat_exit(ctx: TracePointContext) -> u32 {
    match open_events::try_open_exit(
        ctx,
        MONITOR_STAT_OPENAT_EVENTS_EMITTED,
        MONITOR_STAT_OPENAT_RINGBUF_DROPPED,
    ) {
        Ok(v) => v,
        Err(v) => v,
    }
}

#[tracepoint]
pub fn assay_monitor_openat2_exit(ctx: TracePointContext) -> u32 {
    match open_events::try_open_exit(
        ctx,
        MONITOR_STAT_OPENAT2_EVENTS_EMITTED,
        MONITOR_STAT_OPENAT2_RINGBUF_DROPPED,
    ) {
        Ok(v) => v,
        Err(v) => v,
    }
}

#[tracepoint]
pub fn assay_monitor_connect(ctx: TracePointContext) -> u32 {
    match connect_events::try_connect(ctx) {
        Ok(v) => v,
        Err(v) => v,
    }
}

#[tracepoint]
pub fn assay_monitor_sendto(ctx: TracePointContext) -> u32 {
    match connect_events::try_sendto(ctx) {
        Ok(v) => v,
        Err(v) => v,
    }
}

#[tracepoint]
pub fn assay_monitor_sendmsg(ctx: TracePointContext) -> u32 {
    match connect_events::try_sendmsg(ctx) {
        Ok(v) => v,
        Err(v) => v,
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    unsafe { core::hint::unreachable_unchecked() }
}

#[tracepoint]
pub fn assay_monitor_fork(ctx: TracePointContext) -> u32 {
    match fork_events::try_fork(ctx) {
        Ok(v) => v,
        Err(v) => v,
    }
}
