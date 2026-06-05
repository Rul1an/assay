use super::*;

#[repr(C)]
#[derive(Clone, Copy)]
struct UserMsghdrHead {
    msg_name: u64,
    msg_namelen: u32,
    _pad: u32,
}

pub(super) fn try_connect(ctx: TracePointContext) -> Result<u32, u32> {
    if !is_monitored() {
        return Ok(0);
    }

    // Dynamic offset resolution
    let sockaddr_offset = unsafe { CONFIG.get(&KEY_OFFSET_SOCKADDR) }
        .map(|v| *v as usize)
        .unwrap_or(DEFAULT_OFFSET as usize);

    let sockaddr_ptr: u64 = unsafe { ctx.read_at(sockaddr_offset).map_err(|_| 1u32)? };
    emit_sockaddr_event(
        sockaddr_ptr,
        EVENT_CONNECT,
        MONITOR_STAT_CONNECT_EVENTS_EMITTED,
        MONITOR_STAT_CONNECT_RINGBUF_DROPPED,
        NON_IP_FAMILY_STAT_DISABLED,
    )
}

pub(super) fn try_sendto(ctx: TracePointContext) -> Result<u32, u32> {
    if !is_monitored() {
        return Ok(0);
    }

    let sockaddr_offset = unsafe { CONFIG.get(&KEY_OFFSET_SENDTO_SOCKADDR) }
        .map(|v| *v as usize)
        .unwrap_or(48);
    let sockaddr_ptr: u64 = unsafe { ctx.read_at(sockaddr_offset).map_err(|_| 1u32)? };
    if sockaddr_ptr == 0 {
        // Address-less send (e.g. a connected socket): no destination sockaddr in
        // this call, so the peer is not recoverable here. Socket type is not
        // classified. Count it instead of dropping silently.
        inc_stat(MONITOR_STAT_SENDTO_NO_PEER);
        return Ok(0);
    }
    emit_sockaddr_event(
        sockaddr_ptr,
        EVENT_SENDTO,
        MONITOR_STAT_SENDTO_EVENTS_EMITTED,
        MONITOR_STAT_SENDTO_RINGBUF_DROPPED,
        MONITOR_STAT_SENDTO_NON_IP_FAMILY,
    )
}

pub(super) fn try_sendmsg(ctx: TracePointContext) -> Result<u32, u32> {
    if !is_monitored() {
        return Ok(0);
    }

    let msghdr_offset = unsafe { CONFIG.get(&KEY_OFFSET_SENDMSG_MSGHDR) }
        .map(|v| *v as usize)
        .unwrap_or(DEFAULT_OFFSET as usize);
    let msghdr_ptr: u64 = unsafe { ctx.read_at(msghdr_offset).map_err(|_| 1u32)? };
    if msghdr_ptr == 0 {
        return Ok(0);
    }

    let msghdr = unsafe {
        aya_ebpf::helpers::bpf_probe_read_user(msghdr_ptr as *const UserMsghdrHead)
            .map_err(|_| 1u32)?
    };
    if msghdr.msg_name == 0 || msghdr.msg_namelen == 0 {
        // Address-less send (e.g. a connected socket): the message carries no
        // destination address, so the peer is not recoverable here. Socket type
        // is not classified. Count it instead of dropping.
        inc_stat(MONITOR_STAT_SENDMSG_NO_PEER);
        return Ok(0);
    }

    emit_sockaddr_event(
        msghdr.msg_name,
        EVENT_SENDMSG,
        MONITOR_STAT_SENDMSG_EVENTS_EMITTED,
        MONITOR_STAT_SENDMSG_RINGBUF_DROPPED,
        MONITOR_STAT_SENDMSG_NON_IP_FAMILY,
    )
}

/// Sentinel for `emit_sockaddr_event`'s `non_ip_family_stat`: do not count a
/// non-IP family skip (used by the connect path, where this is plain plumbing).
const NON_IP_FAMILY_STAT_DISABLED: u32 = u32::MAX;

#[inline(always)]
fn emit_sockaddr_event(
    sockaddr_ptr: u64,
    event_type: u32,
    emitted_stat: u32,
    dropped_stat: u32,
    non_ip_family_stat: u32,
) -> Result<u32, u32> {
    if sockaddr_ptr == 0 {
        return Ok(0);
    }

    // We can't easily read indefinite structs, so we read a fixed chunk (e.g. 128 bytes)
    // to cover sockaddr_in / sockaddr_in6.
    let mut raw_sockaddr = [0u8; 128];
    unsafe {
        let _ = aya_ebpf::helpers::bpf_probe_read_user(sockaddr_ptr as *const [u8; 128])
            .map(|x| raw_sockaddr = x);
    }

    // Runner-spike only normalizes IPv4/IPv6 endpoints into attribution
    // evidence. AF_UNIX and other family telemetry is runtime plumbing, so skip
    // it before reserving tracepoint ring buffer space. For sendto/sendmsg we
    // count the skip (non_ip_family_stat) so the datagram peer label stays
    // honest; the connect path passes the disabled sentinel.
    let family = u16::from_ne_bytes([raw_sockaddr[0], raw_sockaddr[1]]);
    if family != 2 && family != 10 {
        if non_ip_family_stat != NON_IP_FAMILY_STAT_DISABLED {
            inc_stat(non_ip_family_stat);
        }
        return Ok(0);
    }

    if let Some(mut entry) = EVENTS.reserve::<MonitorEvent>(0) {
        let ev = entry.as_mut_ptr() as *mut MonitorEvent;
        unsafe {
            write_event_header(ev, current_tgid(), event_type);

            // Copy pre-read stack buffer into ringbuf payload
            let data_ptr = (*ev).data.as_mut_ptr();
            let n = if raw_sockaddr.len() < DATA_LEN {
                raw_sockaddr.len()
            } else {
                DATA_LEN
            };
            core::ptr::copy_nonoverlapping(raw_sockaddr.as_ptr(), data_ptr, n);
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
