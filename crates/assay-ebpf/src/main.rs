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

use assay_common::{
    MonitorEvent, EVENT_CONNECT, EVENT_OPENAT, KEY_DEDUP_OPEN_PATHS, KEY_MONITOR_ALL,
    MONITOR_STATS_LEN, MONITOR_STAT_CONNECT_EVENTS_EMITTED, MONITOR_STAT_CONNECT_RINGBUF_DROPPED,
    MONITOR_STAT_OPENAT2_EVENTS_EMITTED, MONITOR_STAT_OPENAT2_RINGBUF_DROPPED,
    MONITOR_STAT_OPENAT_EVENTS_EMITTED, MONITOR_STAT_OPENAT_RINGBUF_DROPPED,
    MONITOR_STAT_TRACEPOINT_EVENTS_EMITTED, MONITOR_STAT_TRACEPOINT_RINGBUF_DROPPED,
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

#[tracepoint]
pub fn assay_monitor_openat_exit(ctx: TracePointContext) -> u32 {
    match try_open_exit(
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
    match try_open_exit(
        ctx,
        MONITOR_STAT_OPENAT2_EVENTS_EMITTED,
        MONITOR_STAT_OPENAT2_RINGBUF_DROPPED,
    ) {
        Ok(v) => v,
        Err(v) => v,
    }
}

fn try_openat2(ctx: TracePointContext) -> Result<u32, u32> {
    if let Some(hits) = TP_HIT.get_ptr_mut(0) {
        unsafe { *hits += 1 };
    }

    let how_offset = unsafe { CONFIG.get(&KEY_OFFSET_OPENAT2_HOW) }
        .map(|v| *v as usize)
        .unwrap_or(32);
    let how_ptr: u64 = unsafe { ctx.read_at(how_offset).map_err(|_| 1u32)? };
    let mut flags = 0u64;
    let mut mode = 0u64;
    let mut resolve = 0u64;
    if how_ptr != 0 {
        if let Ok(how) =
            unsafe { aya_ebpf::helpers::bpf_probe_read_user(how_ptr as *const vmlinux::open_how) }
        {
            flags = how.flags;
            mode = how.mode;
            resolve = how.resolve;
        }
    }

    store_open_pending(ctx, KEY_OFFSET_FILENAME_OPENAT2, flags, mode, resolve)
}

fn try_openat(ctx: TracePointContext) -> Result<u32, u32> {
    if let Some(hits) = TP_HIT.get_ptr_mut(0) {
        unsafe { *hits += 1 };
    }

    let flags_offset = unsafe { CONFIG.get(&KEY_OFFSET_OPENAT_FLAGS) }
        .map(|v| *v as usize)
        .unwrap_or(32);
    let mode_offset = unsafe { CONFIG.get(&KEY_OFFSET_OPENAT_MODE) }
        .map(|v| *v as usize)
        .unwrap_or(40);
    let flags: i32 = unsafe { ctx.read_at(flags_offset).unwrap_or(0) };
    let mode: u64 = unsafe { ctx.read_at::<u64>(mode_offset).unwrap_or(0) };

    store_open_pending(ctx, KEY_OFFSET_FILENAME, flags as u64, mode, 0)
}

#[inline(always)]
fn store_open_pending(
    ctx: TracePointContext,
    offset_key: u32,
    flags: u64,
    mode: u64,
    resolve: u64,
) -> Result<u32, u32> {
    if !is_monitored() {
        return Ok(0);
    }

    let filename_offset = unsafe { CONFIG.get(&offset_key) }
        .map(|v| *v as usize)
        .unwrap_or(DEFAULT_OFFSET as usize);

    let filename_ptr: u64 = unsafe { ctx.read_at(filename_offset).map_err(|_| 1u32)? };
    if filename_ptr == 0 {
        return Ok(0);
    }

    let pending = match OPEN_SCRATCH.get_ptr_mut(0) {
        Some(pending) => pending,
        None => return Ok(0),
    };
    unsafe {
        core::ptr::write_bytes((*pending).data.as_mut_ptr(), 0, DATA_LEN);
        (*pending).flags = flags;
        (*pending).mode = mode;
        (*pending).resolve = resolve;
    }
    let read_result = unsafe {
        aya_ebpf::helpers::bpf_probe_read_user_str_bytes(
            filename_ptr as *const u8,
            &mut (*pending).data,
        )
    };
    if read_result.is_err() {
        return Ok(0);
    }
    let path = unsafe { &(*pending).data };
    if is_loader_telemetry_open_path(path) {
        return Ok(0);
    }
    if should_dedup_open_path(path) {
        return Ok(0);
    }

    let key = bpf_get_current_pid_tgid();
    let _ = unsafe { PENDING_OPEN.insert(&key, &*pending, 0) };

    Ok(0)
}

#[inline(always)]
fn try_open_exit(ctx: TracePointContext, emitted_stat: u32, dropped_stat: u32) -> Result<u32, u32> {
    let key = bpf_get_current_pid_tgid();
    let pending = match unsafe { PENDING_OPEN.get(&key) } {
        Some(pending) => pending,
        None => return Ok(0),
    };
    let _ = PENDING_OPEN.remove(&key);

    let ret_offset = unsafe { CONFIG.get(&KEY_OFFSET_SYSCALL_EXIT_RET) }
        .map(|v| *v as usize)
        .unwrap_or(16);
    let ret: i64 = unsafe { ctx.read_at(ret_offset).unwrap_or(0) };

    if let Some(mut entry) = EVENTS.reserve::<MonitorEvent>(0) {
        let ev = entry.as_mut_ptr() as *mut MonitorEvent;
        unsafe {
            write_event_header(ev, current_tgid(), EVENT_OPENAT);
            (*ev).flags = pending.flags;
            (*ev).mode = pending.mode;
            (*ev).resolve = pending.resolve;
            (*ev).return_value = ret;
            core::ptr::copy_nonoverlapping(
                pending.data.as_ptr(),
                (*ev).data.as_mut_ptr(),
                DATA_LEN,
            );
        }
        entry.submit(0);
        inc_stat(MONITOR_STAT_TRACEPOINT_EVENTS_EMITTED);
        inc_stat(emitted_stat);
    } else {
        inc_stat(MONITOR_STAT_TRACEPOINT_RINGBUF_DROPPED);
        inc_stat(dropped_stat);
    }

    Ok(0)
}

#[inline(always)]
fn should_dedup_open_path(path: &[u8; DATA_LEN]) -> bool {
    let dedup = unsafe { CONFIG.get(&KEY_DEDUP_OPEN_PATHS) }
        .copied()
        .unwrap_or(0)
        != 0;
    if !dedup {
        return false;
    }

    let key = hash_open_path(path);
    if unsafe { OPEN_PATH_SEEN.get(&key) }.is_some() {
        return true;
    }
    let seen = 1u8;
    let _ = OPEN_PATH_SEEN.insert(&key, &seen, 0);
    false
}

#[inline(always)]
fn hash_open_path(path: &[u8; DATA_LEN]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for index in 0..DATA_LEN {
        let byte = path[index];
        if byte == 0 {
            break;
        }
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3u64);
    }
    hash
}

#[inline(always)]
fn is_loader_telemetry_open_path(path: &[u8; DATA_LEN]) -> bool {
    // Dynamic linker and libc config probes flooded delegated runs without
    // carrying runner-spike attribution evidence.
    bytes_start_with(path, b"/etc/ld.so.cache\0")
        || bytes_start_with(path, b"/etc/localtime\0")
        || bytes_start_with(path, b"/etc/ssl/openssl.cnf\0")
        // Python runtime bootstrap files are control-plane noise from the MCP
        // fixture process, not agent file access.
        || bytes_start_with(path, b"/usr/pyvenv.cfg\0")
        // System library and locale lookups dominated the openat stream and
        // varied with loader state across otherwise identical fixtures.
        || bytes_start_with(path, b"/lib/")
        || bytes_start_with(path, b"/lib32/")
        || bytes_start_with(path, b"/lib64/")
        || bytes_start_with(path, b"/usr/bin/pyvenv.cfg\0")
        || bytes_start_with(path, b"/usr/bin/python3._pth\0")
        || bytes_start_with(path, b"/usr/bin/python3.12._pth\0")
        || bytes_start_with(path, b"/usr/bin/pybuilddir.txt\0")
        // The OpenAI Agents fixture's vendored dependency tree is SDK runtime
        // plumbing; SDK evidence is recorded from the normalized SDK layer.
        || bytes_start_with(
            path,
            b"/opt/actions-runner/_work/assay/assay/runner-fixtures/openai-agents/node_modules",
        )
        || bytes_start_with(path, b"/usr/local/lib/")
        || bytes_start_with(path, b"/usr/local/share/locale/")
        || bytes_start_with(path, b"/usr/lib/")
        || bytes_start_with(path, b"/usr/share/locale/")
        // Kernel and device introspection paths are monitor/runtime plumbing,
        // not filesystem capability evidence for the fixture.
        || bytes_start_with(path, b"/proc/")
        || bytes_start_with(path, b"/sys/")
        || bytes_start_with(path, b"/dev/")
}

#[inline(always)]
fn bytes_start_with(path: &[u8; DATA_LEN], prefix: &[u8]) -> bool {
    for index in 0..DATA_LEN {
        if index >= prefix.len() {
            return true;
        }
        if path[index] != prefix[index] {
            return false;
        }
    }
    false
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

    // Runner-spike only normalizes IPv4/IPv6 endpoints into attribution
    // evidence. AF_UNIX and other connect telemetry is runtime plumbing, so
    // skip it before reserving tracepoint ring buffer space.
    let family = u16::from_ne_bytes([raw_sockaddr[0], raw_sockaddr[1]]);
    if family != 2 && family != 10 {
        return Ok(0);
    }

    if let Some(mut entry) = EVENTS.reserve::<MonitorEvent>(0) {
        let ev = entry.as_mut_ptr() as *mut MonitorEvent;
        unsafe {
            write_event_header(ev, current_tgid(), EVENT_CONNECT);

            // Copy pre-read stack buffer into ringbuf payload
            let data_ptr = (*ev).data.as_mut_ptr();
            let n = if raw_sockaddr.len() < DATA_LEN {
                raw_sockaddr.len()
            } else {
                DATA_LEN
            };
            core::ptr::copy_nonoverlapping(raw_sockaddr.as_ptr(), data_ptr, n);
        }
        entry.submit(0);
        inc_stat(MONITOR_STAT_TRACEPOINT_EVENTS_EMITTED);
        inc_stat(MONITOR_STAT_CONNECT_EVENTS_EMITTED);
    } else {
        inc_stat(MONITOR_STAT_TRACEPOINT_RINGBUF_DROPPED);
        inc_stat(MONITOR_STAT_CONNECT_RINGBUF_DROPPED);
    }

    Ok(0)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
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
        inc_stat(MONITOR_STAT_TRACEPOINT_EVENTS_EMITTED);
    } else {
        inc_stat(MONITOR_STAT_TRACEPOINT_RINGBUF_DROPPED);
    }

    Ok(0)
}
