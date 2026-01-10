use crate::MonitorError;
use assay_common::MonitorEvent;
// ReceiverStream is used in type alias, kept.
use tokio_stream::wrappers::ReceiverStream;

pub type EventStream = ReceiverStream<Result<MonitorEvent, MonitorError>>;

thread_local! {
    // Per-thread scratch buffer (optional). Not strictly necessary.
    static _SCRATCH: core::cell::Cell<u8> = core::cell::Cell::new(0);
}

/// Parse fixed-size MonitorEvent from ringbuf bytes.
/// Safe pattern: MaybeUninit + memcpy into repr(C) POD.
pub fn parse_event(bytes: &[u8]) -> Result<MonitorEvent, MonitorError> {
    let need = core::mem::size_of::<MonitorEvent>();
    if bytes.len() < need {
        return Err(MonitorError::InvalidEvent {
            got: bytes.len(),
            need,
        });
    }

    // SAFETY:
    // - MonitorEvent is #[repr(C)] and contains only Copy POD fields.
    // - We copy exactly `size_of::<MonitorEvent>()` bytes into the struct.
    // - Layout is protected by compile-time asserts in assay-common.
    let mut out = core::mem::MaybeUninit::<MonitorEvent>::uninit();
    unsafe {
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), out.as_mut_ptr() as *mut u8, need);
        Ok(out.assume_init())
    }
}

/// Interpret `event.data` as a C-style nul-terminated string slice.
///
/// Useful for EVENT_OPENAT payloads where eBPF writes a path into `data`.
/// Returns an error if the string is not nul-terminated within DATA_LEN.
pub fn cstr_from_data(data: &[u8]) -> Result<&str, MonitorError> {
    // Find first NUL byte
    let nul_pos = data
        .iter()
        .position(|&b| b == 0)
        .ok_or(MonitorError::InvalidEvent {
            got: data.len(),
            need: 1, // "needs a NUL terminator"
        })?;

    core::str::from_utf8(&data[..nul_pos]).map_err(|_| MonitorError::InvalidEvent {
        got: data.len(),
        need: data.len(),
    })
}

#[cfg(target_os = "linux")]
pub fn send_parsed(
    tx: &tokio::sync::mpsc::Sender<Result<MonitorEvent, MonitorError>>,
    data: &[u8],
) {
    let ev = parse_event(data);
    // best-effort send
    let _ = tx.blocking_send(ev);
}

