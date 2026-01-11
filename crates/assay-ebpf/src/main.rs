#![no_std]
#![no_main]

use assay_common::{MonitorEvent, EVENT_CONNECT, EVENT_OPENAT};
use aya_ebpf::{
    macros::{map, tracepoint},
    maps::{HashMap, RingBuf},
    programs::TracePointContext,
};

#[map]
static EVENTS: RingBuf = RingBuf::with_byte_size(256 * 1024, 0);

#[map]
static MONITORED_PIDS: HashMap<u32, u8> = HashMap::with_max_entries(1024, 0);

#[inline(always)]
fn current_tgid() -> u32 {
    (aya_ebpf::helpers::bpf_get_current_pid_tgid() >> 32) as u32
}

const DATA_LEN: usize = 256;

#[inline(always)]
unsafe fn write_event_header(ev: *mut MonitorEvent, pid: u32, event_type: u32) {
    (*ev).pid = pid;
    (*ev).event_type = event_type;
    // Zero payload in-place
    core::ptr::write_bytes((*ev).data.as_mut_ptr(), 0, (*ev).data.len());
}

#[tracepoint]
pub fn assay_monitor_openat(ctx: TracePointContext) -> u32 {
    match try_openat(ctx) {
        Ok(v) => v,
        Err(v) => v,
    }
}

fn try_openat(ctx: TracePointContext) -> Result<u32, u32> {
    let tgid = current_tgid();

    if unsafe { MONITORED_PIDS.get(&tgid) }.is_none() {
        return Ok(0);
    }

    // filename is the 2nd argument (offset 24 for x86_64)
    // const char *filename
    const FILENAME_OFFSET: usize = 24;
    let filename_ptr: u64 = unsafe { ctx.read_at(FILENAME_OFFSET).map_err(|_| 1u32)? };

    if let Some(mut entry) = EVENTS.reserve::<MonitorEvent>(0) {
        let ev = entry.as_mut_ptr() as *mut MonitorEvent;
        unsafe {
            write_event_header(ev, tgid, EVENT_OPENAT);

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
    match try_connect(ctx) {
        Ok(v) => v,
        Err(v) => v,
    }
}

fn try_connect(ctx: TracePointContext) -> Result<u32, u32> {
    let tgid = current_tgid();

    if unsafe { MONITORED_PIDS.get(&tgid) }.is_none() {
        return Ok(0);
    }

    // sockaddr is the 2nd argument (offset 24 for x86_64)
    // struct sockaddr *uservaddr
    const SOCKADDR_OFFSET: usize = 24;
    let sockaddr_ptr: u64 = unsafe { ctx.read_at(SOCKADDR_OFFSET).map_err(|_| 1u32)? };

    // We can't easily read indefinite structs, so we read a fixed chunk (e.g. 128 bytes)
    // to cover sockaddr_in / sockaddr_in6.
    let mut raw_sockaddr = [0u8; 128];
    unsafe {
        let _ = aya_ebpf::helpers::bpf_probe_read_user(sockaddr_ptr as *const [u8; 128])
            .map(|x| raw_sockaddr = x);
    }

    if let Some(mut entry) = EVENTS.reserve::<MonitorEvent>(0) {
        let ev = entry.as_mut_ptr() as *mut MonitorEvent;
        unsafe {
            write_event_header(ev, tgid, EVENT_CONNECT);

            // Copy pre-read stack buffer into ringbuf payload
            let data_ptr = (*ev).data.as_mut_ptr();
            let n = if raw_sockaddr.len() < DATA_LEN { raw_sockaddr.len() } else { DATA_LEN };
            core::ptr::copy_nonoverlapping(raw_sockaddr.as_ptr(), data_ptr, n);
        }
        entry.submit(0);
    }

    Ok(0)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    unsafe { core::hint::unreachable_unchecked() }
}
