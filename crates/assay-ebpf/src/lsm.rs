use aya_ebpf::{
    helpers::{bpf_get_current_cgroup_id, bpf_get_current_pid_tgid, bpf_probe_read_kernel},
    macros::{lsm, map},
    maps::{Array, HashMap, RingBuf},
    programs::LsmContext,
};
use crate::{MONITORED_CGROUPS, CONFIG, KEY_MONITOR_ALL};
use core::ffi::c_void;

// Use generated bindings
use crate::vmlinux::{file, inode, super_block};

const MAX_DENY_PATHS: u32 = 256;

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

// Helper to emit events matching MonitorEvent ABI
fn emit_event(event_id: u32, _cgroup_id: u64, _rule_id: u32, data: &[u8], _path_len: u32) {
    // MonitorEvent Layout (assay-common):
    // offset 0: pid (u32)
    // offset 4: event_type (u32)
    // offset 8: data ([u8; 512])

    if let Some(mut entry) = LSM_EVENTS.reserve::<[u8; 520]>(0) {
        let buf = entry.as_mut_ptr() as *mut u8;
        unsafe {
            // Write PID
            let pid = (bpf_get_current_pid_tgid() >> 32) as u32;
            *(buf as *mut u32) = pid;

            // Write Event Type
            *(buf.add(4) as *mut u32) = event_id;

            // Write Data
            // We strip cgroup_id/rule_id/path_len headers for now to match
            // the simple decoder in cli/monitor.rs which expects payload at data[0].
            // (We can re-add them as a header inside data later if needed).
            let data_ptr = buf.add(8);
            let len = if data.len() > 512 { 512 } else { data.len() };
            core::ptr::copy_nonoverlapping(data.as_ptr(), data_ptr, len);

            // Zero pad the rest if needed, but not strictly required for viewing
             if len < 512 {
                core::ptr::write_bytes(data_ptr.add(len), 0, 512 - len);
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

    // DEBUG: Hook Entry (108)
    let monitor_val = unsafe { CONFIG.get(&KEY_MONITOR_ALL).copied().unwrap_or(0) };
    let mut dbg_entry = [0u8; 64];
    unsafe { *(dbg_entry.as_mut_ptr() as *mut u64) = monitor_val as u64 };

    let monitor_all = monitor_val != 0;
    if !monitor_all && unsafe { MONITORED_CGROUPS.get(&cgroup_id).is_none() } {
        return Ok(0);
    }

    emit_event(108, cgroup_id, 0, &dbg_entry, 0);

    // CO-RE Inode Resolution using generated bindings
    let f = file_ptr as *const file;

    // Read f_inode
    let inode_ptr = unsafe {
        bpf_probe_read_kernel(&((*f).f_inode) as *const *mut inode).unwrap_or(core::ptr::null_mut())
    };

    if inode_ptr.is_null() {
         let mut err_data = [0u8; 64];
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
        // s_dev is typically u32 (dev_t)
        // Check local bindings to confirm type if necessary, usually it matches.
        s_dev = unsafe {
            bpf_probe_read_kernel(&((*sb_ptr).s_dev) as *const u32).unwrap_or(0)
        };
    }

    // Event 112: Inode Resolved
    let mut ino_data = [0u8; 64];
    unsafe {
        *(ino_data.as_mut_ptr() as *mut u64) = s_dev as u64;
        *(ino_data.as_mut_ptr().add(8) as *mut u64) = i_ino;
    }
    emit_event(112, cgroup_id, 0, &ino_data, 0);

    Ok(0)
}
