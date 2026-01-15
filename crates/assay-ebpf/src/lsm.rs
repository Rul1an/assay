use aya_ebpf::{
    macros::{lsm, map},
    maps::{HashMap, RingBuf, Array},
    programs::LsmContext,
    helpers::{bpf_get_current_cgroup_id, bpf_ktime_get_ns, bpf_get_current_pid_tgid},
};
use crate::MONITORED_CGROUPS;
use core::ffi::c_void;

#[map]
static CONFIG_LSM: HashMap<u32, u32> = HashMap::with_max_entries(16, 0);

const MAX_DENY_PATHS: u32 = 256;
const MAX_PATH_LEN: usize = 256;

const EVENT_FILE_BLOCKED: u32 = 10;
const EVENT_FILE_ALLOWED: u32 = 11;

#[map]
static DENY_PATHS_EXACT: HashMap<u64, u32> = HashMap::with_max_entries(MAX_DENY_PATHS, 0);

#[repr(C)]
#[derive(Clone, Copy)]
struct DenyPrefix {
    prefix_len: u32,
    rule_id: u32,
}

#[map]
static DENY_PATHS_PREFIX: HashMap<u64, DenyPrefix> = HashMap::with_max_entries(MAX_DENY_PATHS, 0);

#[map]
static LSM_EVENTS: RingBuf = RingBuf::with_byte_size(128 * 1024, 0);

#[map]
static LSM_STATS: Array<u64> = Array::with_max_entries(8, 0);

const STAT_CHECKS: u32 = 0;
const STAT_BLOCKED: u32 = 1;
const STAT_ALLOWED: u32 = 2;
const STAT_ERRORS: u32 = 3;

#[repr(C)]
struct LsmEvent {
    event_type: u32,
    pid: u32,
    timestamp_ns: u64,
    cgroup_id: u64,
    rule_id: u32,
    action: u32,
    path: [u8; MAX_PATH_LEN],
    path_len: u32,
}

#[lsm(hook = "file_open")]
pub fn file_open_lsm(ctx: LsmContext) -> i32 {
    match try_file_open(&ctx) {
        Ok(result) => result,
        Err(_) => {
            inc_stat(STAT_ERRORS);
            0
        }
    }
}

#[inline(always)]
#[inline(always)]
fn try_file_open(ctx: &LsmContext) -> Result<i32, i64> {
    inc_stat(STAT_CHECKS);

    let file_ptr: *const c_void = unsafe { ctx.arg(0) };
    if file_ptr.is_null() {
        return Ok(0);
    }

    let cgroup_id = unsafe { bpf_get_current_cgroup_id() };

    let monitor_val = unsafe { CONFIG_LSM.get(&0).copied().unwrap_or(0) };
    let monitor_all = monitor_val != 0;

    if !monitor_all && unsafe { MONITORED_CGROUPS.get(&cgroup_id).is_none() } {
        return Ok(0);
    }

    // Reserve space in RingBuf FIRST to avoid stack allocation
    let mut event_reservation = LSM_EVENTS.reserve::<LsmEvent>(0);
    if event_reservation.is_none() {
        inc_stat(STAT_ERRORS);
        // Fail-open if monitoring buffer full (standard practice to avoid breaking system)
        return Ok(0);
    }

    // We have a reservation. Using `unwrap_unchecked` pattern safely because of check above.
    let mut event = event_reservation.unwrap();
    let ev = unsafe { &mut *event.as_mut_ptr() };

    // Initialize fixed fields
    ev.pid = (bpf_get_current_pid_tgid() >> 32) as u32;
    ev.timestamp_ns = unsafe { bpf_ktime_get_ns() };
    ev.cgroup_id = cgroup_id;

    // Read path DIRECTLY into ringbuf memory
    let mut path_len = match read_file_path(file_ptr, &mut ev.path) {
        Ok(len) => len,
        Err(_) => {
            event.discard(0);
            return Ok(0);
        }
    };

    if path_len > MAX_PATH_LEN {
        path_len = MAX_PATH_LEN;
    }

    // Align with userspace hashing (no null terminator)
    if path_len > 0 && ev.path[path_len - 1] == 0 {
        path_len -= 1;
    }
    ev.path_len = path_len as u32;

    if path_len == 0 {
         event.discard(0);
         return Ok(0);
    }

    // Zero trailing bytes to prevent info leaks (manual memset)
    // This is verifier-friendly as it depends on `path_len` which is bounded.
    // Optimization: Unroll or chunk? Simple loop for now.
    // Note: bpf_d_path wrote up to path_len.
    // Using a bounded loop for safety.
    let start = path_len;
    if start < MAX_PATH_LEN {
        // We can't use a dynamic range iterator easily in BPF sometimes.
        // But `for i in start..MAX_PATH_LEN` works if compiler handles it.
        // To be safe, we just leave it for now or rely on read_file_path behavior?
        // `read_file_path` writes `path_len` bytes.
        // We MUST zero the rest.
        // Let's use a explicit slice assignment if possible?
        // `ev.path[start..].fill(0)`? No std lib.

        // Use a simple loop.
        for i in 0..MAX_PATH_LEN {
             if i >= start {
                 ev.path[i] = 0;
             }
        }
    }

    let path_hash = fnv1a_hash(&ev.path[..path_len]);

    let mut ret = 0;

    if let Some(&rule_id) = unsafe { DENY_PATHS_EXACT.get(&path_hash) } {
        ev.event_type = EVENT_FILE_BLOCKED;
        ev.rule_id = rule_id;
        ev.action = 0; // Deny? action=0 meant BLOCKED in emit_event usage?
                       // Wait, emit_event call passed '0' for action in BLOCKED case?
                       // Line 103: emit_event(..., 0).
                       // Line 108: emit_event(..., 1).
        ev.action = 0;
        inc_stat(STAT_BLOCKED);
        ret = -1; // -EPERM
    } else {
        ev.event_type = EVENT_FILE_ALLOWED;
        ev.rule_id = 0;
        ev.action = 1;
        inc_stat(STAT_ALLOWED);
    }

    event.submit(0);
    Ok(ret)
}

#[inline(always)]
fn fnv1a_hash(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &b in data {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[inline(always)]
fn inc_stat(index: u32) {
    if let Some(val) = LSM_STATS.get_ptr_mut(index) {
        unsafe { *val += 1 };
    }
}

use aya_ebpf::bindings::path;

// Minimal file struct shim for typed field access
// Heuristic based on Lima 6.17.0-8-generic BTF: f_path starts at offset 64.
#[repr(C)]
struct file_shim {
    _pad: [u8; 64],
    f_path: path,
}

#[inline(always)]
fn read_file_path(file_ptr: *const c_void, buf: &mut [u8]) -> Result<usize, i64> {
    use aya_ebpf::helpers::bpf_d_path;

    if file_ptr.is_null() {
        return Ok(0);
    }

    // Use typed access to satisfy verifier type tracking for bpf_d_path
    let f = unsafe { &*(file_ptr as *const file_shim) };
    let path_ptr = &f.f_path as *const path as *mut path;

    // Use *mut i8 cast strictly
    let len = unsafe {
        bpf_d_path(path_ptr, buf.as_mut_ptr() as *mut i8, MAX_PATH_LEN as u32)
    };

    if len < 0 {
        return Err(len as i64);
    }

    Ok(len as usize)
}
