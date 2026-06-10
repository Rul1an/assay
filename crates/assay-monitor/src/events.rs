use crate::MonitorError;
use assay_common::MonitorEvent;
// ReceiverStream is used in type alias, kept.
use tokio_stream::wrappers::ReceiverStream;

pub type EventStream = ReceiverStream<Result<MonitorEvent, MonitorError>>;

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
    #[allow(unsafe_code)] // Performance: zero-copy parse from ringbuf
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_event_rejects_short_record_with_got_and_need() {
        // A stale eBPF object emits the previous 40 + 480 = 520 byte layout while userspace now
        // expects 40 + 512 = 552. The parser must reject the short record (fail-closed), reporting
        // the observed and expected sizes, rather than decoding a truncated struct.
        let need = core::mem::size_of::<MonitorEvent>();
        assert_eq!(
            need, 552,
            "MonitorEvent ABI size changed; update the stale-object test"
        );
        let stale = vec![0u8; 520];
        match parse_event(&stale) {
            Err(MonitorError::InvalidEvent { got, need: n }) => {
                assert_eq!(got, 520);
                assert_eq!(n, 552);
            }
            other => panic!("expected InvalidEvent for a 520-byte record, got {other:?}"),
        }
    }

    #[test]
    fn parse_event_accepts_exact_size_record() {
        let ev = parse_event(&vec![0u8; core::mem::size_of::<MonitorEvent>()]);
        assert!(ev.is_ok(), "a record of the exact pinned size must parse");
    }
}
