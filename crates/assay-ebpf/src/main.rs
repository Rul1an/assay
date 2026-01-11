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

const DATA_LEN: usize = 256;

unsafe fn write_event_header(ev: *mut MonitorEvent, pid: u32, event_type: u8) {
    (*ev).pid = pid;
    (*ev).event_type = event_type;
    // Zero payload in-place
    core::ptr::write_bytes((*ev).data.as_mut_ptr(), 0, (*ev).data.len());
}

#[tracepoint]
pub fn assay_monitor_openat(ctx: TracePointContext) -> u32 {
    match try_assay_monitor_openat(ctx) {
        Ok(ret) => ret,
        Err(ret) => ret,
    }
}

fn try_assay_monitor_openat(ctx: TracePointContext) -> Result<u32, u32> {
    let pid = (ctx.pid() >> 32) as u32;

    if unsafe { MONITORED_PIDS.get(&pid) }.is_none() {
        return Ok(0);
    }

    // filename is the 2nd argument (offset 24 for x86_64)
    // const char *filename
    const FILENAME_OFFSET: usize = 24;
    let filename_ptr: u64 = unsafe { ctx.read_at(FILENAME_OFFSET).map_err(|_| 1u32)? };

    if let Some(mut entry) = EVENTS.reserve::<MonitorEvent>(0) {
        let ev = entry.as_mut_ptr() as *mut MonitorEvent;
        unsafe {
            write_event_header(ev, pid, EVENT_OPENAT);

            // Safe slice construction from raw pointer to avoid implicit autoref issues
            let data_ptr = (*ev).data.as_mut_ptr();
            let data_len = (*ev).data.len();
            let data = core::slice::from_raw_parts_mut(data_ptr, data_len);

            let _ = aya_ebpf::helpers::bpf_probe_read_user_str_bytes(
                filename_ptr as *const u8,
                data,
            );
        }
        entry.submit(0);
    }

    Ok(0)
}

#[tracepoint]
pub fn assay_monitor_connect(ctx: TracePointContext) -> u32 {
    match try_assay_monitor_connect(ctx) {
        Ok(ret) => ret,
        Err(ret) => ret,
    }
}

fn try_assay_monitor_connect(ctx: TracePointContext) -> Result<u32, u32> {
    let pid = (ctx.pid() >> 32) as u32;

    if unsafe { MONITORED_PIDS.get(&pid) }.is_none() {
        return Ok(0);
    }

    // sockaddr is the 2nd argument (offset 24 for x86_64)
    // struct sockaddr *uservaddr
    const SOCKADDR_OFFSET: usize = 24;
    let sockaddr_ptr: u64 = unsafe { ctx.read_at(SOCKADDR_OFFSET).map_err(|_| 1u32)? };

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
