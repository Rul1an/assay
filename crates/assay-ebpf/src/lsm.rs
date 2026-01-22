use aya_ebpf::{
    helpers::{bpf_get_current_cgroup_id, bpf_get_current_pid_tgid, bpf_probe_read_kernel},
    macros::{lsm, map},
    maps::{Array, HashMap, RingBuf},
    programs::LsmContext,
};
use crate::{MONITORED_CGROUPS, CONFIG, KEY_MONITOR_ALL, LSM_HIT, LSM_DENY, DENY_INO, LSM_EVENTS, STATS};
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

    // CO-RE Inode Resolution:
    // We use bpf_probe_read_kernel to read pointers safely.
    // The "CO-RE" magic happens because we are casting to pointers of `vmlinux::file`/`inode`
    // which are generated with BTF relocations enabled.
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

    // Enforcement Check
    // 1. Exact Match (if gen != 0 or strictly enforced)
    // Enforcement Check
    // 1. Exact Match (if gen != 0 or strictly enforced)
    if i_gen != 0 {
        let key_exact = assay_common::InodeKeyMap {
            ino: i_ino,
            dev: s_dev,
            gen: i_gen,
        };
        if let Some(rule_id) = unsafe { DENY_INO.get(&key_exact) } {
            unsafe { aya_ebpf::helpers::bpf_printk!(b"LSM: BLOCKED %llu:%llu (Exact Gen %u) rule=%u\0", s_dev as u64, i_ino, i_gen, *rule_id); }

            if let Some(denies) = LSM_DENY.get_ptr_mut(0) {
                unsafe { *denies += 1 };
            }

            let mut alert_data = [0u8; 64];
            unsafe {
                *(alert_data.as_mut_ptr() as *mut u64) = s_dev as u64;
                *(alert_data.as_mut_ptr().add(8) as *mut u64) = i_ino;
                *(alert_data.as_mut_ptr().add(16) as *mut u32) = *rule_id;
            }
            emit_event(&ctx, 10, cgroup_id, *rule_id, &alert_data, 0);
            return Err(-1); // EPERM matched exact
        }
    }

    // 2. Fallback Match (Gen 0 / Unknown)
    // This catches cases where userspace couldn't resolve generation (e.g. tmpfs or failed ioctl)
    // but correctly resolved dev/ino.
    let key_fallback = assay_common::InodeKeyMap {
        ino: i_ino,
        dev: s_dev,
        gen: 0,
    };

    if let Some(rule_id) = unsafe { DENY_INO.get(&key_fallback) } {
        unsafe { aya_ebpf::helpers::bpf_printk!(b"LSM: BLOCKED %llu:%llu (Fallback Gen) rule=%u\0", s_dev as u64, i_ino, *rule_id); }

        if let Some(denies) = LSM_DENY.get_ptr_mut(0) {
            unsafe { *denies += 1 };
        }

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

    // ABI: s_dev(u64) | i_ino(u64) | i_gen(u32)
    // Use copy_from_slice for safety (avoids pointer casts/unaligned writes)
    let dev_bytes = (s_dev as u64).to_ne_bytes();
    ino_data[0..8].copy_from_slice(&dev_bytes);

    let ino_bytes = (i_ino as u64).to_ne_bytes();
    ino_data[8..16].copy_from_slice(&ino_bytes);

    let gen_bytes = (i_gen as u32).to_ne_bytes();
    ino_data[16..20].copy_from_slice(&gen_bytes);

    emit_event(&ctx, 112, cgroup_id, 0, &ino_data, 0);

    Ok(0)
}
