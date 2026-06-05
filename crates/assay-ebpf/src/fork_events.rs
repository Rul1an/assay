use super::*;

pub(super) fn try_fork(ctx: TracePointContext) -> Result<u32, u32> {
    // Only trace if parent is monitored.
    // NOTE: Cgroup inheritance means child is AUTOMATICALLY in the cgroup.
    // So if parent is in cgroup, child is too.
    // We check `is_monitored()` which checks current (parent) cgroup.
    if !is_monitored() {
        return Ok(0);
    }

    // SAFETY: CONFIG is an eBPF map owned by this program. Missing keys fall
    // back to the fork tracepoint ABI defaults below.
    let parent_offset = unsafe { CONFIG.get(&KEY_OFFSET_FORK_PARENT) }
        .map(|v| *v as usize)
        .unwrap_or(24); // Common default for parent_pid

    // SAFETY: CONFIG is an eBPF map owned by this program. Missing keys fall
    // back to the fork tracepoint ABI defaults below.
    let child_offset = unsafe { CONFIG.get(&KEY_OFFSET_FORK_CHILD) }
        .map(|v| *v as usize)
        .unwrap_or(44); // Common default for child_pid

    // SAFETY: The offset comes from configured tracepoint ABI state; failed
    // reads are converted into the existing error path.
    let parent_pid: u32 = unsafe { ctx.read_at(parent_offset).map_err(|_| 1u32)? };
    // SAFETY: The offset comes from configured tracepoint ABI state; failed
    // reads are converted into the existing error path.
    let child_pid: u32 = unsafe { ctx.read_at(child_offset).map_err(|_| 1u32)? };

    if let Some(mut entry) = EVENTS.reserve::<MonitorEvent>(0) {
        let ev = entry.as_mut_ptr();
        // SAFETY: `ev` points to a reserved `MonitorEvent` ring-buffer entry.
        // Header and child pid payload are initialized before submit.
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
