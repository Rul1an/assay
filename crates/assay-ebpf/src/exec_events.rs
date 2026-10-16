use super::*;
use assay_common::{exec_record_offset_in_bounds, EVENT_EXEC, KEY_OFFSET_EXEC_FILENAME};
use aya_ebpf::{helpers::bpf_probe_read_kernel_str_bytes, EbpfContext};

/// `sched_process_exec` fires once the new image has replaced the old one, so this records a
/// successful exec only. A failed execve never reaches it.
pub(super) fn try_exec(ctx: TracePointContext) -> Result<u32, u32> {
    if !is_monitored() {
        return Ok(0);
    }

    // SAFETY: CONFIG is an eBPF map owned by this program. There is no fallback offset: the
    // loader resolves it from the running kernel's format file when it attaches this probe, or
    // refuses to attach.
    let loc_offset = match unsafe { CONFIG.get(&KEY_OFFSET_EXEC_FILENAME) } {
        Some(offset) => *offset,
        None => return Ok(0),
    };
    // The loader refuses an out-of-bounds offset with this same predicate, so this only guards
    // a CONFIG map written by something other than the loader's attach-time configuration.
    if !exec_record_offset_in_bounds(loc_offset) {
        return Ok(0);
    }

    // SAFETY: The offset comes from configured tracepoint ABI state and is bounded above;
    // failed reads are converted into the existing error path.
    let loc: u32 = unsafe { ctx.read_at(loc_offset as usize).map_err(|_| 1u32)? };
    // `__data_loc`: low 16 bits are the payload offset within the record, high 16 its length.
    // A descriptor the kernel wrote outside these bounds is skipped and not counted anywhere.
    let data_offset = loc & 0xffff;
    let data_len = loc >> 16;
    if data_len == 0 || data_offset < loc_offset + 4 || !exec_record_offset_in_bounds(data_offset) {
        return Ok(0);
    }

    if let Some(mut entry) = EVENTS.reserve::<MonitorEvent>(0) {
        let ev = entry.as_mut_ptr();
        // SAFETY: `ev` points to a reserved `MonitorEvent` ring-buffer entry. The header and a
        // zeroed payload are written first; the string read copies at most `DATA_LEN - 1` bytes
        // plus a terminator into that payload, so a longer filename is truncated, never overrun.
        let read_ok = unsafe {
            write_event_header(ev, current_tgid(), EVENT_EXEC);
            let src = (ctx.as_ptr() as *const u8).add(data_offset as usize);
            bpf_probe_read_kernel_str_bytes(src, &mut (*ev).data).is_ok()
        };
        // A failed string read discards the entry; like a skipped descriptor, it is not counted.
        if !read_ok {
            entry.discard(0);
            return Ok(0);
        }
        entry.submit(0);
        inc_stat(MONITOR_STAT_TRACEPOINT_EVENTS_EMITTED);
    } else {
        inc_stat(MONITOR_STAT_TRACEPOINT_RINGBUF_DROPPED);
    }

    Ok(0)
}
