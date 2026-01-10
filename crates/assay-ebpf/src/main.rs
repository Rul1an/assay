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

        slot.write(MonitorEvent {
            pid: tgid,
            event_type: EVENT_OPENAT,
            data: [0u8; 256],
        });

        let event: &mut MonitorEvent = unsafe { slot.assume_init_mut() };

        unsafe {
            let _ = aya_ebpf::helpers::bpf_probe_read_user_str_bytes(
                filename_ptr as *const u8,
                &mut event.data,
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

    // sys_connect(fd, uservaddr, addrlen)
    // uservaddr is 2nd arg -> offset 24
    const SOCKADDR_OFFSET: usize = 24;
    let sockaddr_ptr: u64 = unsafe { ctx.read_at(SOCKADDR_OFFSET).map_err(|_| 1u32)? };

    if let Some(mut entry) = EVENTS.reserve::<MonitorEvent>(0) {
        let slot: &mut MaybeUninit<MonitorEvent> = &mut *entry;

        slot.write(MonitorEvent {
            pid: tgid,
            event_type: EVENT_CONNECT,
            data: [0u8; 256],
        });

        let event: &mut MonitorEvent = unsafe { slot.assume_init_mut() };

        // ✅ Read fixed-size bytes from user pointer
        let raw: [u8; 128] = unsafe {
            aya_ebpf::helpers::bpf_probe_read_user(sockaddr_ptr as *const [u8; 128])
                .map_err(|_| 1u32)?
        };

        event.data[..128].copy_from_slice(&raw);

        entry.submit(0);
    }

    Ok(0)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    unsafe { core::hint::unreachable_unchecked() }
}
