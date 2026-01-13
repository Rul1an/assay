
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

mod vmlinux;
use vmlinux::{file, path};

// Redeclare helper to match our local vmlinux types
#[link_name = "bpf_d_path"]
extern "C" {
    fn bpf_d_path(path: *const path, buf: *mut i8, sz: i32) -> i64;
}

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
fn try_file_open(ctx: &LsmContext) -> Result<i32, i64> {
    inc_stat(STAT_CHECKS);

    let file_ptr: *const c_void = unsafe { ctx.arg(0) };
    if file_ptr.is_null() {
        return Ok(0);
    }

    let cgroup_id = unsafe { bpf_get_current_cgroup_id() };
    let monitor_all = unsafe { CONFIG_LSM.get(&0).copied().unwrap_or(0) != 0 };

    if !monitor_all && unsafe { MONITORED_CGROUPS.get(&cgroup_id).is_none() } {
        return Ok(0);
    }

    let mut path_buf = [0u8; MAX_PATH_LEN];
    let mut path_len = read_file_path(file_ptr, &mut path_buf)?;

    if path_len > MAX_PATH_LEN {
        path_len = MAX_PATH_LEN;
    }

    // Align with userspace hashing (no null terminator)
    if path_len > 0 && path_buf[path_len - 1] == 0 {
        path_len -= 1;
    }

    if path_len == 0 {
        return Ok(0);
    }

    let path_hash = fnv1a_hash(&path_buf[..path_len]);

    if let Some(&rule_id) = unsafe { DENY_PATHS_EXACT.get(&path_hash) } {
        emit_event(EVENT_FILE_BLOCKED, cgroup_id, rule_id, &path_buf, path_len, 0);
        inc_stat(STAT_BLOCKED);
        return Ok(-1); // -EPERM
    }

    emit_event(EVENT_FILE_ALLOWED, cgroup_id, 0, &path_buf, path_len, 1);
    inc_stat(STAT_ALLOWED);
    Ok(0)
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
    if let Some(val) = unsafe { LSM_STATS.get_ptr_mut(index) } {
        unsafe { *val += 1 };
    }
}

#[inline(always)]
fn emit_event(event_type: u32, cgroup_id: u64, rule_id: u32, path: &[u8], path_len: usize, action: u32) {
    if let Some(mut event) = LSM_EVENTS.reserve::<LsmEvent>(0) {
        let ev = unsafe { &mut *event.as_mut_ptr() };
        ev.event_type = event_type;
        ev.pid = (unsafe { bpf_get_current_pid_tgid() } >> 32) as u32;
        ev.timestamp_ns = unsafe { bpf_ktime_get_ns() };
        ev.cgroup_id = cgroup_id;
        ev.rule_id = rule_id;
        ev.action = action;
        ev.path_len = path_len as u32;

        // Optimized copy/clear to avoid verifier loop complexity limit
        let len = if path_len < MAX_PATH_LEN { path_len } else { MAX_PATH_LEN };
        unsafe {
            core::ptr::copy_nonoverlapping(path.as_ptr(), ev.path.as_mut_ptr(), len);
            if len < MAX_PATH_LEN {
                core::ptr::write_bytes(ev.path.as_mut_ptr().add(len), 0, MAX_PATH_LEN - len);
            }
        }
        event.submit(0);
    }
}

#[inline(always)]
fn read_file_path(file_ptr: *const c_void, buf: &mut [u8; MAX_PATH_LEN]) -> Result<usize, i64> {
    let f = file_ptr as *const file;

    // Use addr_of! on the member to preserve CO-RE relocation chain
    // Verifier should now see "PTR_TO_BTF_ID(struct path)"
    let path_ptr = unsafe { core::ptr::addr_of!((*f).f_path) };

    let len = unsafe {
        bpf_d_path(
            path_ptr,
            buf.as_mut_ptr() as *mut i8,
            MAX_PATH_LEN as i32,
        )
    };

    if len < 0 {
        return Err(len);
    }

    Ok(len as usize)
}
