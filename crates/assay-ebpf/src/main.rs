#![no_std]
#![no_main]

use assay_common::{MonitorEvent, EVENT_CONNECT, EVENT_OPENAT};
use aya_ebpf::{
    macros::{map, tracepoint},
    maps::{HashMap, RingBuf},
    programs::TracePointContext,
};
use core::mem::MaybeUninit;

#[map]
static EVENTS: RingBuf = RingBuf::with_byte_size(256 * 1024, 0);

#[map]
static MONITORED_PIDS: HashMap<u32, u8> = HashMap::with_max_entries(1024, 0);

#[tracepoint]
pub fn assay_monitor_openat(ctx: TracePointContext) -> u32 {
    match try_assay_monitor_openat(ctx) {
        Ok(r) => r,
        Err(r) => r,
    }
}

fn try_assay_monitor_openat(ctx: TracePointContext) -> Result<u32, u32> {
    let tgid = (aya_ebpf::helpers::bpf_get_current_pid_tgid() >> 32) as u32;
    if unsafe { MONITORED_PIDS.get(&tgid) }.is_none() {
        return Ok(0);
    }

    const FILENAME_OFFSET: usize = 24;
    let filename_ptr: u64 = unsafe { ctx.read_at(FILENAME_OFFSET).map_err(|_| 1u32)? };

    if let Some(mut entry) = EVENTS.reserve::<MonitorEvent>(0) {
        let slot: &mut MaybeUninit<MonitorEvent> = &mut *entry;
        let ev = slot.as_mut_ptr();

        unsafe {
            // Write scalar fields directly (no stack struct literal)
            (*ev).pid = tgid;
            (*ev).event_type = EVENT_OPENAT;

            // Zero payload in-place (no `[0u8;256]` temp on stack)
            core::ptr::write_bytes((*ev).data.as_mut_ptr(), 0, (*ev).data.len());

            // Fill payload directly into ringbuf memory
            let _ = aya_ebpf::helpers::bpf_probe_read_user_str_bytes(
                filename_ptr as *const u8,
                &mut (*ev).data,
            );
        }

        entry.submit(0);
    }

    Ok(0)
}

#[tracepoint]
pub fn assay_monitor_connect(ctx: TracePointContext) -> u32 {
    match try_assay_monitor_connect(ctx) {
        Ok(r) => r,
        Err(r) => r,
    }
}

fn try_assay_monitor_connect(ctx: TracePointContext) -> Result<u32, u32> {
    let tgid = (aya_ebpf::helpers::bpf_get_current_pid_tgid() >> 32) as u32;
    if unsafe { MONITORED_PIDS.get(&tgid) }.is_none() {
        return Ok(0);
    }

    const SOCKADDR_OFFSET: usize = 24;
    let sockaddr_ptr: u64 = unsafe { ctx.read_at(SOCKADDR_OFFSET).map_err(|_| 1u32)? };

    // Read small, bounded structs onto stack (OK), avoid `[u8;128]`/`[u8;256]` temps.
    const AF_INET: u16 = 2;
    const AF_INET6: u16 = 10;

    #[repr(C)]
    #[derive(Copy, Clone)]
    struct SockAddrIn {
        sin_family: u16,
        sin_port: u16,
        sin_addr: u32,
        sin_zero: [u8; 8],
    }

    #[repr(C)]
    #[derive(Copy, Clone)]
    struct In6Addr {
        s6_addr: [u8; 16],
    }

    #[repr(C)]
    #[derive(Copy, Clone)]
    struct SockAddrIn6 {
        sin6_family: u16,
        sin6_port: u16,
        sin6_flowinfo: u32,
        sin6_addr: In6Addr,
        sin6_scope_id: u32,
    }

    // family read is tiny
    let family: u16 = match unsafe {
        aya_ebpf::helpers::bpf_probe_read_user(sockaddr_ptr as *const u16)
    } {
        Ok(v) => v,
        Err(_) => return Ok(0),
    };

    if let Some(mut entry) = EVENTS.reserve::<MonitorEvent>(0) {
        let slot: &mut MaybeUninit<MonitorEvent> = &mut *entry;
        let ev = slot.as_mut_ptr();

        unsafe {
            (*ev).pid = tgid;
            (*ev).event_type = EVENT_CONNECT;
            core::ptr::write_bytes((*ev).data.as_mut_ptr(), 0, (*ev).data.len());

            // Store family in first 2 bytes (little sanity)
            let fb = family.to_ne_bytes();
            core::ptr::copy_nonoverlapping(fb.as_ptr(), (*ev).data.as_mut_ptr(), 2);

            if family == AF_INET {
                if let Ok(sa) = aya_ebpf::helpers::bpf_probe_read_user(sockaddr_ptr as *const SockAddrIn) {
                    let n = core::mem::size_of::<SockAddrIn>();
                    core::ptr::copy_nonoverlapping(
                        &sa as *const SockAddrIn as *const u8,
                        (*ev).data.as_mut_ptr(),
                        n.min((*ev).data.len()),
                    );
                }
            } else if family == AF_INET6 {
                if let Ok(sa6) = aya_ebpf::helpers::bpf_probe_read_user(sockaddr_ptr as *const SockAddrIn6) {
                    let n = core::mem::size_of::<SockAddrIn6>();
                    core::ptr::copy_nonoverlapping(
                        &sa6 as *const SockAddrIn6 as *const u8,
                        (*ev).data.as_mut_ptr(),
                        n.min((*ev).data.len()),
                    );
                }
            }
            // else: leave payload mostly zeroed (no early return; no leaked reservation)
        }

        entry.submit(0);
    }

    Ok(0)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    unsafe { core::hint::unreachable_unchecked() }
}
