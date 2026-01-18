use aya_ebpf::{
    helpers::{bpf_get_current_cgroup_id, bpf_get_current_pid_tgid, bpf_probe_read_kernel},
    macros::{lsm, map},
    maps::{Array, HashMap, RingBuf},
    programs::LsmContext,
};
use crate::{MONITORED_CGROUPS, CONFIG, KEY_MONITOR_ALL, LSM_HIT, DENY_INO, LSM_EVENTS, STATS};
use core::ffi::c_void;
use crate::vmlinux::{file, inode, super_block};
use aya_log_ebpf::info;

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

// Maps now consolidated in main.rs

// Helper to emit events matching MonitorEvent ABI
fn emit_event(ctx: &LsmContext, event_id: u32, _cgroup_id: u64, _rule_id: u32, data: &[u8], _path_len: u32) {
    if let Some(mut entry) = LSM_EVENTS.reserve::<[u8; 520]>(0) {
        let buf = entry.as_mut_ptr() as *mut u8;
        unsafe {
            // Write PID
            let pid = (bpf_get_current_pid_tgid() >> 32) as u32;
            *(buf as *mut u32) = pid;

            // Write Event Type
            *(buf.add(4) as *mut u32) = event_id;

            // Write Data
            let data_ptr = buf.add(8);
            let len = if data.len() > 512 { 512 } else { data.len() };
            core::ptr::copy_nonoverlapping(data.as_ptr(), data_ptr, len);

            // Zero pad
             if len < 512 {
                core::ptr::write_bytes(data_ptr.add(len), 0, 512 - len);
            }
        }
        entry.submit(0);
        info!(ctx, "LSM Event {} Submitted", event_id);
    }
}

#[lsm(hook = "file_open")]
pub fn file_open_lsm(ctx: LsmContext) -> i32 {
    match try_file_open_lsm(ctx) {
        Ok(ret) => ret,
        Err(ret) => ret as i32,
    }
}

fn try_file_open_lsm(ctx: LsmContext) -> Result<i32, i32> {
    // 0. Mark Hit (Absolute proof kernel reached here)
    if let Some(hits) = LSM_HIT.get_ptr_mut(0) {
        unsafe { *hits += 1 };
    }

    let cgroup_id = unsafe { bpf_get_current_cgroup_id() };

    // Validates that we have a file pointer (arg 0)
    let file_ptr: *const c_void = unsafe { ctx.arg(0) };
    if file_ptr.is_null() {
        return Ok(0);
    }

    let monitor_val = unsafe { CONFIG.get(&KEY_MONITOR_ALL).copied().unwrap_or(0) };
    let monitor_all = monitor_val != 0;

    // Optimization: avoid heavy logic if not monitored
    if !monitor_all && unsafe { MONITORED_CGROUPS.get(&cgroup_id).is_none() } {
        return Ok(0);
    }

    // CO-RE Inode Resolution
    let f = file_ptr as *const file;
    let inode_ptr: *mut inode = unsafe {
        bpf_probe_read_kernel(&((*f).f_inode) as *const *mut inode).unwrap_or(core::ptr::null_mut())
    };

    // Hardening: Null Check
    if inode_ptr.is_null() {
         return Ok(0);
    }

    // Read i_ino
    let i_ino = unsafe { bpf_probe_read_kernel(&((*inode_ptr).i_ino) as *const u64).unwrap_or(0) };

    // Read i_generation (SOTA)
    let i_gen = unsafe { bpf_probe_read_kernel(&((*inode_ptr).i_generation) as *const u32).unwrap_or(0) };

    let sb_ptr: *mut super_block = unsafe { bpf_probe_read_kernel(&((*inode_ptr).i_sb) as *const *mut super_block).unwrap_or(core::ptr::null_mut()) };

    let mut s_dev = 0u32;
    if !sb_ptr.is_null() {
        s_dev = unsafe { bpf_probe_read_kernel(&((*sb_ptr).s_dev) as *const u32).unwrap_or(0) };
    }

    let key = assay_common::InodeKey {
        dev: s_dev,
        pad: 0,
        ino: i_ino,
        gen: 0, // Fallback: Userspace doesn't resolve gen yet (requires ioctl), so we default to 0 for matching.
        _pad2: 0,
    };

    // Diagnostic Printk (DEBUG CLASS 2)
    unsafe {
        aya_ebpf::helpers::bpf_printk!(b"LSM: INODE %llu:%llu\0", s_dev as u64, i_ino);
    }

    // Enforcement Check
    if let Some(rule_id) = unsafe { DENY_INO.get(&key) } {
        unsafe { aya_ebpf::helpers::bpf_printk!(b"LSM: BLOCKED %llu:%llu rule=%u\0", s_dev as u64, i_ino, *rule_id); }

        let mut alert_data = [0u8; 64];
        unsafe {
            *(alert_data.as_mut_ptr() as *mut u64) = s_dev as u64;
            *(alert_data.as_mut_ptr().add(8) as *mut u64) = i_ino;
            *(alert_data.as_mut_ptr().add(16) as *mut u32) = *rule_id;
        }
        emit_event(&ctx, 10, cgroup_id, *rule_id, &alert_data, 0);
        return Err(-1); // EPERM
    }

    // Event 112: Inode Resolved (Telemetry)
    let mut ino_data = [0u8; 64];
    unsafe {
        *(ino_data.as_mut_ptr() as *mut u64) = s_dev as u64;
        *(ino_data.as_mut_ptr().add(8) as *mut u64) = i_ino;
    }
    emit_event(&ctx, 112, cgroup_id, 0, &ino_data, 0);

    Ok(0)
}
