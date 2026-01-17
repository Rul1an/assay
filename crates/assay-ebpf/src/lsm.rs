use aya_ebpf::{
    bindings::t_bpf_context,
    helpers::{bpf_get_current_cgroup_id, bpf_ktime_get_ns, bpf_get_current_pid_tgid, bpf_probe_read_kernel},
    macros::{lsm, map},
    maps::{Array, HashMap, RingBuf},
    programs::LsmContext,
};
// gen import removed
use crate::MONITORED_CGROUPS;
use core::ffi::{c_void, c_char};

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

#[map]
static DUMP_DONE: Array<u32> = Array::with_max_entries(1, 0);

const DATA_LEN: usize = 512;

#[repr(C)]
struct MonitorEvent {
    pid: u32,
    event_type: u32,
    data: [u8; DATA_LEN],
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

#[derive(Clone, Copy)]
#[repr(C)]
pub struct InodeKey {
    pub dev: u64,
    pub ino: u64,
}

#[map]
static DENY_INODES_EXACT: HashMap<InodeKey, u32> = HashMap::with_max_entries(MAX_DENY_PATHS, 0);

#[inline(always)]
fn try_file_open(ctx: &LsmContext) -> Result<i32, i64> {
    inc_stat(STAT_CHECKS);

    let cgroup_id = unsafe { bpf_get_current_cgroup_id() };

     // If this doesn't show, the hook isn't running.
     {
         // DUMP LOGIC REMOVED (Focusing on verifier-safe bpf_d_path)
     }

    let file_ptr: *const c_void = unsafe { ctx.arg(0) };
    if file_ptr.is_null() {
        return Ok(0);
    }

    let monitor_val = unsafe { CONFIG_LSM.get(&0).copied().unwrap_or(0) };
    let monitor_all = monitor_val != 0;

    if !monitor_all && unsafe { MONITORED_CGROUPS.get(&cgroup_id).is_none() } {
        return Ok(0);
    }

    let inode_ptr: *const u8 = core::ptr::null();



    if !inode_ptr.is_null() {
        // 3. Read Inode Fields
        // i_sb at 40 (0x28), i_ino at 64 (0x40)
        let i_sb_addr = (inode_ptr as *const u8).wrapping_add(40) as *const *const c_void;
        let i_ino_addr = (inode_ptr as *const u8).wrapping_add(64) as *const u64;

        let ino = unsafe { bpf_probe_read_kernel(i_ino_addr).unwrap_or(0) };
        let sb_ptr = unsafe { bpf_probe_read_kernel(i_sb_addr).unwrap_or(core::ptr::null()) };

        if !sb_ptr.is_null() {
            // s_dev at 16 (0x10)
            let s_dev_addr = (sb_ptr as *const u8).wrapping_add(16) as *const u32;
            let s_dev = unsafe { bpf_probe_read_kernel(s_dev_addr).unwrap_or(0) };

            if s_dev != 0 {
                let key = InodeKey { dev: s_dev as u64, ino };
                // ... map lookup ...
                 if let Some(&rule_id) = unsafe { DENY_INODES_EXACT.get(&key) } {
                     let partial_path: [u8; MAX_PATH_LEN] = [0; MAX_PATH_LEN];
                     emit_event(EVENT_FILE_BLOCKED, cgroup_id, rule_id, &partial_path[0..0], 0);
                     inc_stat(STAT_BLOCKED);
                     return Ok(-1);
                 }
            }
        }
    }

    // Use MaybeUninit to avoid the expensive zero-initialization loop on stack
    let mut path_buf: [core::mem::MaybeUninit<u8>; MAX_PATH_LEN] =
        unsafe { core::mem::MaybeUninit::uninit().assume_init() };

    // Safety: read_file_path writes to the pointer treating it as *mut u8.
    // It creates a valid C string or fails.
    let buf_ptr = path_buf.as_mut_ptr() as *mut u8;
    let buf_slice = unsafe { core::slice::from_raw_parts_mut(buf_ptr, MAX_PATH_LEN) };

    // CONDITIONAL PATH RESOLUTION
    // Re-enabled with Stack Copy Fix for Verifier
    let mut path_len = read_file_path(file_ptr, buf_slice).unwrap_or(0);

    // ... (Remainder path logic is skipped if len=0) ...
    if path_len == 0 {
        return Ok(0);
    }

    // Null terminate if needed (bpf_d_path usually does, but we ensure sanity)
    // We treat the buffer as a slice of u8 for hash/match
    let path_bytes = &buf_slice[..path_len];

    // Check deny list (Exact)
    let hash = fnv1a_hash(path_bytes);
    if let Some(&rule_id) = unsafe { DENY_PATHS_EXACT.get(&hash) } {
        emit_event(EVENT_FILE_BLOCKED, cgroup_id, rule_id, path_bytes, 0);
        inc_stat(STAT_BLOCKED);
        return Ok(-1);
    }

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
    if let Some(val) = LSM_STATS.get_ptr_mut(index) {
        unsafe { *val += 1 };
    }
}

#[inline(always)]
#[inline(always)]
fn emit_event(event_type: u32, _cgroup_id: u64, _rule_id: u32, path: &[u8], _action: u32) {
    if let Some(mut event) = LSM_EVENTS.reserve::<MonitorEvent>(0) {
        let ev = unsafe { &mut *event.as_mut_ptr() };
        ev.event_type = event_type;
        ev.pid = (bpf_get_current_pid_tgid() >> 32) as u32;

        unsafe {
            // Pack data for Event 100/99 (Debug) specially, or standard packing
            // For now, if event_type == 100, we just assume `path` contains the 16 bytes of debug data.
            // For standard blocking events, we might want to pack cgroup/rule_id?
            // Current userspace expects:
            // Event 100: [dev(8), ino(8)]
            // Event BLOCKED: path string

            if event_type == 100 {
                 let len = if path.len() > 16 { 16 } else { path.len() };
                 core::ptr::copy_nonoverlapping(path.as_ptr(), ev.data.as_mut_ptr(), len);
            } else if event_type == 101 {
                 // Struct Dump: Copy up to 256 bytes (or full slice if larger)
                 let len = if path.len() > 256 { 256 } else { path.len() };
                 core::ptr::copy_nonoverlapping(path.as_ptr(), ev.data.as_mut_ptr(), len);
            } else {
                 // Regular event (File Blocked/Allowed)
                 // Just copy path for now to match userspace expectation for OPENAT-like events
                 // TODO: If we need rule_id, we need to pack it. But userspace monitor.rs line 422 just prints string.
                 let len = if path.len() > DATA_LEN { DATA_LEN } else { path.len() };
                 core::ptr::copy_nonoverlapping(path.as_ptr(), ev.data.as_mut_ptr(), len);
                 // Null terminate if space allows?
                 if len < DATA_LEN {
                     *ev.data.as_mut_ptr().add(len) = 0;
                 }
            }
        }
        event.submit(0);
    }
}

use aya_ebpf::bindings::path;

// Keep read_file_path for future, but it's unused if disabled in try_file_open
// Keep read_file_path for future, but it's unused if disabled in try_file_open
#[inline(always)]
fn read_file_path(file_ptr: *const c_void, buf: &mut [u8]) -> Result<usize, i64> {
   use aya_ebpf::helpers::bpf_d_path;
   if file_ptr.is_null() { return Ok(0); }

   // Heuristic: struct file starts with f_u (rcu_head/callback_head) which is 16 bytes.
   // f_path usually follows immediately at offset 16.
   // POINTER MATH: file_ptr + 16 = address of f_path (struct path) inside struct file.
   // We cast this to *const path to read the DATA of the struct path.
   let f_path_src = unsafe { (file_ptr as *const u8).add(16) as *const path };

   // STACK COPY: We create a local 'struct path' on the stack and copy the kernel data into it.
   // This creates a "safe" object with a known type for the verifier, disconnecting it from
   // the provenance of the file_ptr (which the verifier was confused about).
   let mut local_path: path = unsafe { core::mem::MaybeUninit::zeroed().assume_init() };

   unsafe {
       bpf_probe_read_kernel(f_path_src as *const _, &mut local_path as *mut _ as *mut _)
            .map_err(|e| e as i64)?
   };

   // Now we pass the pointer to our LOCAL stack object to bpf_d_path.
   // The verifier sees: R1 = ptr_to_stack (which contains a struct path).
   // This matches the expected arg type for bpf_d_path.
   let len = unsafe {
       bpf_d_path(&mut local_path as *mut path, buf.as_mut_ptr() as *mut c_char, MAX_PATH_LEN as u32)
   };

   if len < 0 { return Err(len as i64); }
   Ok(len as usize)
}
