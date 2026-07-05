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

/// Project a fixed-size cgroup socket event into the stable monitor event stream.
///
/// The cgroup_sock_addr eBPF programs emit `SocketEvent` records on a separate
/// ring buffer. Userspace normalizes them into `EVENT_CONNECT_BLOCKED`
/// `MonitorEvent`s so downstream evidence exporters consume one event shape.
pub fn parse_socket_event(bytes: &[u8]) -> Result<MonitorEvent, MonitorError> {
    let need = core::mem::size_of::<assay_common::SocketEvent>();
    if bytes.len() < need {
        return Err(MonitorError::InvalidEvent {
            got: bytes.len(),
            need,
        });
    }

    let mut out = MonitorEvent::zeroed();
    out.event_type = u32::from_ne_bytes(bytes[0..4].try_into().expect("slice length checked"));
    out.pid = u32::from_ne_bytes(bytes[4..8].try_into().expect("slice length checked"));

    // Payload ABI consumed by CLI display and runner retained evidence:
    // | cgroup_id u64 | family u16 | port u16 | addr_v4 u32 |
    // | addr_v6 [u8;16] | rule_id u32 | action u32 |.
    out.data[0..8].copy_from_slice(&bytes[16..24]);
    out.data[8..10].copy_from_slice(&bytes[24..26]);
    out.data[10..12].copy_from_slice(&bytes[26..28]);
    out.data[12..16].copy_from_slice(&bytes[28..32]);
    out.data[16..32].copy_from_slice(&bytes[32..48]);
    out.data[32..36].copy_from_slice(&bytes[48..52]);
    out.data[36..40].copy_from_slice(&bytes[52..56]);
    Ok(out)
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

    #[test]
    fn parse_socket_event_projects_blocked_connect_payload() {
        let mut bytes = vec![0_u8; core::mem::size_of::<assay_common::SocketEvent>()];
        bytes[0..4].copy_from_slice(&assay_common::EVENT_CONNECT_BLOCKED.to_ne_bytes());
        bytes[4..8].copy_from_slice(&4242_u32.to_ne_bytes());
        bytes[8..16].copy_from_slice(&123_u64.to_ne_bytes());
        bytes[16..24].copy_from_slice(&99_u64.to_ne_bytes());
        bytes[24..26].copy_from_slice(&2_u16.to_ne_bytes());
        bytes[26..28].copy_from_slice(&443_u16.to_ne_bytes());
        bytes[28..32].copy_from_slice(&[203, 0, 113, 7]);
        bytes[48..52].copy_from_slice(&17_u32.to_ne_bytes());
        bytes[52..56].copy_from_slice(&2_u32.to_ne_bytes());

        let event = parse_socket_event(&bytes).unwrap();

        assert_eq!(event.event_type, assay_common::EVENT_CONNECT_BLOCKED);
        assert_eq!(event.pid, 4242);
        assert_eq!(&event.data[0..8], &99_u64.to_ne_bytes());
        assert_eq!(&event.data[8..10], &2_u16.to_ne_bytes());
        assert_eq!(&event.data[10..12], &443_u16.to_ne_bytes());
        assert_eq!(&event.data[12..16], &[203, 0, 113, 7]);
        assert_eq!(&event.data[32..36], &17_u32.to_ne_bytes());
        assert_eq!(&event.data[36..40], &2_u32.to_ne_bytes());
    }
}
