use super::{path_filter, *};

pub(super) fn try_openat2(ctx: TracePointContext) -> Result<u32, u32> {
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

pub(super) fn try_openat(ctx: TracePointContext) -> Result<u32, u32> {
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
    if path_filter::is_loader_telemetry_open_path(path) {
        return Ok(0);
    }
    if should_dedup_open_path(path, flags) {
        return Ok(0);
    }

    let key = bpf_get_current_pid_tgid();
    let _ = unsafe { PENDING_OPEN.insert(&key, &*pending, 0) };

    Ok(0)
}

#[inline(always)]
pub(super) fn try_open_exit(
    ctx: TracePointContext,
    emitted_stat: u32,
    dropped_stat: u32,
) -> Result<u32, u32> {
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
fn should_dedup_open_path(path: &[u8; DATA_LEN], flags: u64) -> bool {
    let dedup = unsafe { CONFIG.get(&KEY_DEDUP_OPEN_PATHS) }
        .copied()
        .unwrap_or(0)
        != 0;
    if !dedup {
        return false;
    }

    let key = hash_open_path(path, flags);
    if unsafe { OPEN_PATH_SEEN.get(&key) }.is_some() {
        return true;
    }
    let seen = 1u8;
    let _ = OPEN_PATH_SEEN.insert(&key, &seen, 0);
    false
}

#[inline(always)]
fn hash_open_path(path: &[u8; DATA_LEN], flags: u64) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    hash ^= flags;
    hash = hash.wrapping_mul(0x100000001b3u64);
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
