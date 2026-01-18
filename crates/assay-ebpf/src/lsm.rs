use aya_ebpf::{
    helpers::{bpf_get_current_cgroup_id, bpf_ktime_get_ns, bpf_get_current_pid_tgid, bpf_probe_read_kernel},
    macros::{lsm, map},
    maps::{Array, HashMap, RingBuf},
    programs::LsmContext,
};
use crate::{MONITORED_CGROUPS, CONFIG, KEY_MONITOR_ALL};
use core::ffi::{c_void, c_char, c_long};

    // Event 112: Inode Resolved
    let mut ino_data = [0u8; 64];
    unsafe {
        *(ino_data.as_mut_ptr() as *mut u64) = s_dev as u64;
        *(ino_data.as_mut_ptr().add(8) as *mut u64) = i_ino;
    }
    emit_event(112, cgroup_id, 0, &ino_data, 0);

    Ok(0)
}

use crate::{MONITORED_CGROUPS, CONFIG, KEY_MONITOR_ALL};
use core::ffi::{c_void, c_char, c_long};

const MAX_DENY_PATHS: u32 = 256;
const MAX_PATH_LEN: usize = 256;

const EVENT_FILE_BLOCKED: u32 = 10;
const EVENT_FILE_ALLOWED: u32 = 11;

// SOTA: Inode Enforcement Keys
#[repr(C)]
#[derive(Clone, Copy, PreserveAccessIndex)]
pub struct super_block {
    pub s_dev: u32,
}

#[repr(C)]
#[derive(Clone, Copy, PreserveAccessIndex)]
pub struct inode {
    pub i_ino: u64,
    pub i_sb: *mut super_block,
}

#[repr(C)]
#[derive(Clone, Copy, PreserveAccessIndex)]
pub struct file {
    pub f_inode: *mut inode,
}

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
static LSM_EVENTS: RingBuf = RingBuf::with_byte_size(1024 * 1024, 0); // Increased buffer

// Statistics
#[map]
static STATS: Array<u32> = Array::with_max_entries(10, 0);

fn inc_stat(idx: u32) {
    if let Some(val) = STATS.get_ptr_mut(idx) {
        unsafe { *val += 1 };
    }
}

// Helper to emit events
fn emit_event(event_id: u32, cgroup_id: u64, rule_id: u32, data: &[u8], path_len: u32) {
    // Layout:
    // u32 event_id
    // u64 cgroup_id
    // u32 rule_id
    // u32 path_len
    // [u8; 64] data (payload)

    // Using a fixed size struct for RingBuf reservation might be safer,
    // but here we pack manually.
    if let Some(mut entry) = LSM_EVENTS.reserve::<[u8; 84]>(0) {
        let buf = entry.as_mut_ptr() as *mut u8;
        unsafe {
            *(buf as *mut u32) = event_id;
            *(buf.add(4) as *mut u64) = cgroup_id;
            *(buf.add(12) as *mut u32) = rule_id;
            *(buf.add(16) as *mut u32) = path_len; // length or extra check

            // data (max 64 bytes)
            let data_ptr = buf.add(20);
            let len = if data.len() > 64 { 64 } else { data.len() };
            core::ptr::copy_nonoverlapping(data.as_ptr(), data_ptr, len);
            // zero pad
            if len < 64 {
                core::ptr::write_bytes(data_ptr.add(len), 0, 64 - len);
            }
        }
        entry.submit(0);
    }
}

#[lsm(hook = "file_open")]
pub fn file_open_lsm(ctx: LsmContext) -> i32 {
    match try_file_open_lsm(ctx) {
        Ok(ret) => ret,
        Err(ret) => ret as i32,
    }
}

fn try_file_open_lsm(ctx: LsmContext) -> Result<i32, i64> {
    let cgroup_id = unsafe { bpf_get_current_cgroup_id() };

    // Validates that we have a file pointer (arg 0)
    let file_ptr: *const c_void = unsafe { ctx.arg(0) };
    if file_ptr.is_null() {
        return Ok(0);
    }

    // We do NOT use bpf_get_current_comm here to avoid build errors and overhead.
    // Instead we rely on the monitor check momentarily,
    // BUT we emit a debug event 108 unconditionally first (careful with flood).

    // DEBUG: Hook Entry (108)
    // Payload: monitor_all flag (u64)
    let monitor_val = unsafe { CONFIG.get(&KEY_MONITOR_ALL).copied().unwrap_or(0) };
    let mut dbg_entry = [0u8; 64];
    unsafe { *(dbg_entry.as_mut_ptr() as *mut u64) = monitor_val as u64 };

    // Check monitor flag
    let monitor_all = monitor_val != 0;
    if !monitor_all && unsafe { MONITORED_CGROUPS.get(&cgroup_id).is_none() } {
        return Ok(0);
    }

    emit_event(108, cgroup_id, 0, &dbg_entry, 0);

    // CO-RE Inode Resolution
    // Instead of offsets 56/32, we verify logic by reading inode and dev.
    let f = file_ptr as *const file;

    // Read f_inode
    let inode_ptr = unsafe {
        bpf_probe_read_kernel(&((*f).f_inode) as *const *mut inode).unwrap_or(core::ptr::null_mut())
    };

    if inode_ptr.is_null() {
         let mut err_data = [0u8; 64]; // Event 106 reusing ID for inode null
         emit_event(106, cgroup_id, 0, &err_data, 0);
         return Ok(0);
    }

    // Read i_ino
    let i_ino = unsafe {
        bpf_probe_read_kernel(&((*inode_ptr).i_ino) as *const u64).unwrap_or(0)
    };

    // Read i_sb
    let sb_ptr = unsafe {
        bpf_probe_read_kernel(&((*inode_ptr).i_sb) as *const *mut super_block).unwrap_or(core::ptr::null_mut())
    };

    let mut s_dev = 0u32;
    if !sb_ptr.is_null() {
        s_dev = unsafe {
            bpf_probe_read_kernel(&((*sb_ptr).s_dev) as *const u32).unwrap_or(0)
        };
    }

    // Event 112: Inode Resolved
    // Payload: [dev(u64), ino(u64)]
    let mut ino_data = [0u8; 64];
    unsafe {
        *(ino_data.as_mut_ptr() as *mut u64) = s_dev as u64;
        *(ino_data.as_mut_ptr().add(8) as *mut u64) = i_ino;
    }
    emit_event(112, cgroup_id, 0, &ino_data, 0);

    Ok(0)
}

// gen import removed
use crate::{MONITORED_CGROUPS, CONFIG, KEY_MONITOR_ALL};
use core::ffi::{c_void, c_char};

// CONFIG_LSM removed in favor of shared CONFIG

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

    let file_ptr: *const c_void = unsafe { ctx.arg(0) };
    if file_ptr.is_null() {
        return Ok(0);
    }

    // Filter for "cat" only to debug
    let mut comm = [0u8; 16];
    let _ = unsafe { bpf_get_current_comm(&mut comm as *mut _ as *mut c_void, 16u32) };
    // "cat" is [99, 97, 116, 0]
    let is_cat = comm[0] == 99 && comm[1] == 97 && comm[2] == 116 && comm[3] == 0;

    if !is_cat {
        return Ok(0);
    }

    let monitor_val = unsafe { CONFIG.get(&KEY_MONITOR_ALL).copied().unwrap_or(0) };

    // DEBUG: Hook Entry
    let mut dbg = [0u8; 64];
    dbg[0] = monitor_val as u8;
    emit_event(108, cgroup_id, 0, &dbg, 0);
    let monitor_all = monitor_val != 0;

    // if !monitor_all && unsafe { MONITORED_CGROUPS.get(&cgroup_id).is_none() } {
    //    return Ok(0);
    // }

    // DEBUG: Passed Monitor Check
    let mut dbg = [0u8; 64];
    emit_event(109, cgroup_id, 0, &dbg, 0);

    // --- MANUAL PATH RESOLUTION (OFFSET GUESSING) ---
    let dentry_ptr_loc = unsafe { (file_ptr as *const u8).add(56) };
    let dentry_ptr_val: u64 = unsafe {
        bpf_probe_read_kernel(dentry_ptr_loc as *const u64).unwrap_or(0)
    };

    // DEBUG: Read Dentry
    let mut dbg = [0u8; 64];
    dbg[0..8].copy_from_slice(&dentry_ptr_val.to_ne_bytes());
    emit_event(110, cgroup_id, 0, &dbg, 0);

    if dentry_ptr_val != 0 {
        let dentry_ptr = dentry_ptr_val as *const u8;

        // Offset 32 for d_name.name
        let name_ptr_loc = unsafe { dentry_ptr.add(32) };
        let name_ptr_val: u64 = unsafe {
             bpf_probe_read_kernel(name_ptr_loc as *const u64).unwrap_or(0)
        };

        // DEBUG: Read Name Ptr
        let mut dbg = [0u8; 64];
        dbg[0..8].copy_from_slice(&name_ptr_val.to_ne_bytes());
        emit_event(111, cgroup_id, 0, &dbg, 0);

        if name_ptr_val != 0 {
            let name_ptr = name_ptr_val as *const u8;

            // Read Name
            let mut name_buf = [0u8; 64];
            let _ = unsafe {
                bpf_probe_read_kernel(name_ptr as *const [u8; 64]).map(|b| name_buf = b)
            };

            // Emit Event 105 (Resolved Name)
            emit_event(105, cgroup_id, 0, &name_buf, 0);
        } else {
            // Event 107
             let mut dbg = [0u8; 64];
             emit_event(107, cgroup_id, 0, &dbg, 0);
        }
    } else {
        // Event 106
        let mut dbg = [0u8; 64];
        emit_event(106, cgroup_id, 0, &dbg, 0);
    }
    // -------------------------------

    // Disable Path Resolution for now (Verifier issue with bpf_d_path stack ptr)
    /*
    let mut path_buf: [core::mem::MaybeUninit<u8>; MAX_PATH_LEN] =
        unsafe { core::mem::MaybeUninit::uninit().assume_init() };
    let buf_ptr = path_buf.as_mut_ptr() as *mut u8;
    let buf_slice = unsafe { core::slice::from_raw_parts_mut(buf_ptr, MAX_PATH_LEN) };

    let path_len = read_file_path(file_ptr, buf_slice).unwrap_or(0);
    if path_len == 0 {
        return Ok(0);
    }

    let path_bytes = &buf_slice[..path_len];
    let hash = fnv1a_hash(path_bytes);
    if let Some(&rule_id) = unsafe { DENY_PATHS_EXACT.get(&hash) } {
        emit_event(EVENT_FILE_BLOCKED, cgroup_id, rule_id, path_bytes, 0);
        inc_stat(STAT_BLOCKED);
        return Ok(-1);
    }
    */

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
// #[inline(always)] removed
fn emit_event(event_type: u32, _cgroup_id: u64, _rule_id: u32, path: &[u8], _action: u32) {
    if let Some(mut event) = LSM_EVENTS.reserve::<MonitorEvent>(0) {
        let ev = unsafe { &mut *event.as_mut_ptr() };
        ev.event_type = event_type;
        ev.pid = (bpf_get_current_pid_tgid() >> 32) as u32;

        unsafe {
            // Pack data for Event 100/99 (Debug) specially, or standard packing
            if event_type == 100 {
                 let len = if path.len() > 16 { 16 } else { path.len() };
                 core::ptr::copy_nonoverlapping(path.as_ptr(), ev.data.as_mut_ptr(), len);
            } else if event_type == 101 {
                 // Struct Dump: Copy up to 256 bytes (or full slice if larger)
                 let len = if path.len() > 256 { 256 } else { path.len() };
                 core::ptr::copy_nonoverlapping(path.as_ptr(), ev.data.as_mut_ptr(), len);
            } else {
                 // Regular event (File Blocked/Allowed)
                 let len = if path.len() > DATA_LEN { DATA_LEN } else { path.len() };
                 core::ptr::copy_nonoverlapping(path.as_ptr(), ev.data.as_mut_ptr(), len);
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
#[inline(always)]
fn read_file_path(file_ptr: *const c_void, buf: &mut [u8]) -> Result<usize, i64> {
   /*
   use aya_ebpf::helpers::bpf_d_path;
   if file_ptr.is_null() { return Ok(0); }

   // Heuristic: struct file starts with f_u (rcu_head/callback_head) which is 16 bytes.
   // f_path usually follows immediately at offset 16.
   // POINTER MATH: file_ptr + 16 = address of f_path (struct path) inside struct file.
   // We cast this to *const path to read the DATA of the struct path.
   // verify file_ptr + 16 offset logic
   let f_path_src = unsafe { (file_ptr as *const u8).add(16) as *const aya_ebpf::bindings::path };

   // STACK COPY: Read the struct path from kernel memory directly onto the stack.
   // bpf_probe_read_kernel returns the value read.
   let mut local_path: aya_ebpf::bindings::path = unsafe {
       bpf_probe_read_kernel(f_path_src).map_err(|e| e as i64)?
   };

   // Now we pass the pointer to our LOCAL stack object to bpf_d_path.
   // buf.as_mut_ptr() is *mut u8. On ARM64 (Linux), c_char is u8.
   // We cast to *mut c_char (u8) to match the signature.
   // FIXED: Cast to *mut i8 to satisfy the signature if needed
   let len = unsafe {
       bpf_d_path(&mut local_path as *mut aya_ebpf::bindings::path, buf.as_mut_ptr() as *mut i8, MAX_PATH_LEN as u32)
   };

   if len < 0 { return Err(len as i64); }
   Ok(len as usize)
   */
   Ok(0)
}
