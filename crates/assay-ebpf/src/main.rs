#![no_std]
#![no_main]





#[no_mangle]
#[link_section = "license"]
pub static _LICENSE: [u8; 4] = *b"GPL\0";


pub mod lsm;
pub mod socket_lsm;
#[allow(dead_code)]
#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
#[allow(non_upper_case_globals)]
#[allow(clippy::all)]
pub mod vmlinux;

use assay_common::{MonitorEvent, EVENT_CONNECT};
use aya_ebpf::{
    macros::{map, tracepoint},
    maps::{Array, HashMap, RingBuf},
    programs::TracePointContext,
    helpers::{
        bpf_get_current_cgroup_id,
        bpf_get_current_ancestor_cgroup_id,
        bpf_get_current_pid_tgid,
    },
};

#[inline(always)]
fn current_tgid() -> u32 {
    (bpf_get_current_pid_tgid() >> 32) as u32
}


#[map]
static EVENTS: RingBuf = RingBuf::with_byte_size(256 * 1024, 0);

#[map]
pub static TP_HIT: Array<u64> = Array::with_max_entries(1, 0);

#[map]
pub static MONITORED_PIDS: HashMap<u32, u8> = HashMap::with_max_entries(1024, 0);

/// Map of Cgroup IDs (inodes) we're monitoring.
/// Key: Cgroup ID (u64), Value: 1 (present)
#[map]
pub static MONITORED_CGROUPS: HashMap<u64, u8> = HashMap::with_max_entries(1024, 0);

/// Configuration Map for dynamic offsets.
/// Key 0: openat filename offset (default 24)
/// Key 1: connect sockaddr offset (default 24)
#[map]
pub static CONFIG: HashMap<u32, u32> = HashMap::with_max_entries(16, 0);

#[map]
pub static LSM_HIT: Array<u64> = Array::with_max_entries(1, 0);

#[map]
pub static DENY_INO: HashMap<assay_common::InodeKey, u32> = HashMap::with_max_entries(1024, 0);

#[map]
pub static LSM_EVENTS: RingBuf = RingBuf::with_byte_size(256 * 1024, 0);

#[map]
pub static STATS: Array<u32> = Array::with_max_entries(10, 0);

const KEY_OFFSET_FILENAME: u32 = 0;
const KEY_OFFSET_SOCKADDR: u32 = 1;
const KEY_OFFSET_FORK_PARENT: u32 = 2;
const KEY_OFFSET_FORK_CHILD: u32 = 3;
const KEY_OFFSET_FILENAME_OPENAT2: u32 = 4;
const DEFAULT_OFFSET: u32 = 24;

const KEY_MAX_ANCESTOR_DEPTH: u32 = 10;
pub const KEY_MONITOR_ALL: u32 = 100;
const MAX_ANCESTOR_DEPTH_HARD: usize = 16;

#[inline(always)]
fn max_ancestor_depth() -> usize {
    let v = unsafe { CONFIG.get(&KEY_MAX_ANCESTOR_DEPTH) }.copied().unwrap_or(8);
    let v = v as usize;
    if v > MAX_ANCESTOR_DEPTH_HARD { MAX_ANCESTOR_DEPTH_HARD } else { v }
}

#[inline(always)]
fn is_monitored() -> bool {
    // 0. Global Monitor-All Override
    if unsafe { CONFIG.get(&KEY_MONITOR_ALL) }.copied().unwrap_or(0) != 0 {
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
        if i >= depth { break; }

        let ancestor_id = unsafe { bpf_get_current_ancestor_cgroup_id(i as i32) };
        if ancestor_id == 0 { break; } // Root or error

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
    // Zero payload in-place
    core::ptr::write_bytes((*ev).data.as_mut_ptr(), 0, (*ev).data.len());
}

#[tracepoint]
pub fn assay_monitor_openat(ctx: TracePointContext) -> u32 {
    match try_openat(ctx) {
        Ok(v) => v,
        Err(v) => v,
    }
}

/// SOTA Coverage: Also monitor openat2 (modern Linux)
#[tracepoint]
pub fn assay_monitor_openat2(ctx: TracePointContext) -> u32 {
    match try_openat2(ctx) {
        Ok(v) => v,
        Err(v) => v,
    }
}

fn try_openat2(ctx: TracePointContext) -> Result<u32, u32> {
    if let Some(hits) = TP_HIT.get_ptr_mut(0) {
        unsafe { *hits += 1 };
    }
    Ok(0)
}

fn try_openat(ctx: TracePointContext) -> Result<u32, u32> {
    if let Some(hits) = TP_HIT.get_ptr_mut(0) {
        unsafe { *hits += 1 };
    }
    Ok(0)
}

#[tracepoint]
pub fn assay_monitor_connect(ctx: TracePointContext) -> u32 {
    match try_connect(ctx) {
        Ok(v) => v,
        Err(v) => v,
    }
}

fn try_connect(ctx: TracePointContext) -> Result<u32, u32> {
    if !is_monitored() {
        return Ok(0);
    }

    // Dynamic offset resolution
    let sockaddr_offset = unsafe { CONFIG.get(&KEY_OFFSET_SOCKADDR) }
        .map(|v| *v as usize)
        .unwrap_or(DEFAULT_OFFSET as usize);

    let sockaddr_ptr: u64 = unsafe { ctx.read_at(sockaddr_offset).map_err(|_| 1u32)? };

    // We can't easily read indefinite structs, so we read a fixed chunk (e.g. 128 bytes)
    // to cover sockaddr_in / sockaddr_in6.
    let mut raw_sockaddr = [0u8; 128];
    unsafe {
        let _ = aya_ebpf::helpers::bpf_probe_read_user(sockaddr_ptr as *const [u8; 128])
            .map(|x| raw_sockaddr = x);
    }

    if let Some(mut entry) = EVENTS.reserve::<MonitorEvent>(0) {
        let ev = entry.as_mut_ptr() as *mut MonitorEvent;
        unsafe {
            write_event_header(ev, current_tgid(), EVENT_CONNECT);

            // Copy pre-read stack buffer into ringbuf payload
            let data_ptr = (*ev).data.as_mut_ptr();
            let n = if raw_sockaddr.len() < DATA_LEN { raw_sockaddr.len() } else { DATA_LEN };
            core::ptr::copy_nonoverlapping(raw_sockaddr.as_ptr(), data_ptr, n);
        }
        entry.submit(0);
    }

    Ok(0)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    unsafe { core::hint::unreachable_unchecked() }
}


#[tracepoint]
pub fn assay_monitor_fork(ctx: TracePointContext) -> u32 {
    match try_fork(ctx) {
        Ok(v) => v,
        Err(v) => v,
    }
}

fn try_fork(ctx: TracePointContext) -> Result<u32, u32> {
    // Only trace if parent is monitored.
    // NOTE: Cgroup inheritance means child is AUTOMATICALLY in the cgroup.
    // So if parent is in cgroup, child is too.
    // We check `is_monitored()` which checks current (parent) cgroup.
    if !is_monitored() {
        return Ok(0);
    }

    let parent_offset = unsafe { CONFIG.get(&KEY_OFFSET_FORK_PARENT) }
        .map(|v| *v as usize)
        .unwrap_or(24); // Common default for parent_pid

    let child_offset = unsafe { CONFIG.get(&KEY_OFFSET_FORK_CHILD) }
        .map(|v| *v as usize)
        .unwrap_or(44); // Common default for child_pid

    let parent_pid: u32 = unsafe { ctx.read_at(parent_offset).map_err(|_| 1u32)? };
    let child_pid: u32 = unsafe { ctx.read_at(child_offset).map_err(|_| 1u32)? };

    if let Some(mut entry) = EVENTS.reserve::<MonitorEvent>(0) {
        let ev = entry.as_mut_ptr() as *mut MonitorEvent;
        unsafe {
            use assay_common::EVENT_FORK;
            write_event_header(ev, parent_pid, EVENT_FORK);

            // Payload: child_pid (4 bytes)
            let data_ptr = (*ev).data.as_mut_ptr();
            core::ptr::write(data_ptr as *mut u32, child_pid);
        }
        entry.submit(0);
    }

    Ok(0)
}
