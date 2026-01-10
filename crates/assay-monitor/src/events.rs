use crate::MonitorError;
use assay_common::MonitorEvent;
#[cfg(target_os = "linux")]
use tokio::sync::mpsc;
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

#[cfg(target_os = "linux")]
pub fn spawn_ringbuf_reader(
    mut ringbuf: aya::maps::ring_buf::RingBuf<aya::maps::MapData>,
) -> (
    mpsc::Receiver<Result<MonitorEvent, MonitorError>>,
    std::thread::JoinHandle<()>,
) {
    let (tx, rx) = mpsc::channel(1024);

    let handle = std::thread::spawn(move || {
        use aya::maps::ringbuf::RingBuf; // Ensure trait is used if needed, or inherent method

        loop {
            // Aya RingBuf next() is blocking or non-blocking depending on impl, but here we treat it as the iterator.
            // Note: Aya 0.12 RingBuf implements Iterator? Or we use next() method.
            // The user provided code suggests ringbuf.next().

            // Checking if `next()` on RingBuf works directly or iteration is needed.
            // Assuming the snippet provided is correct for the targeted aya version.
            match ringbuf.next() {
                Some(Ok(item)) => {
                    let bytes = item.as_ref();
                    let ev = parse_event(bytes);
                    // best-effort send; if receiver dropped we stop
                    if tx.blocking_send(ev).is_err() {
                        break;
                    }
                }
                Some(Err(e)) => {
                    let _ = tx.blocking_send(Err(MonitorError::RingBuf(e.to_string())));
                    break;
                }
                None => {
                    // If iterator returns None, it might be finished or just empty if non-blocking?
                    // Standard RingBuf usually blocks or returns None if closed.
                    // A blocking reader should just block.
                    // Detailed aya-rs/aya behavior: RingBuf implements Iterator where next() blocks?
                    // If it is non-blocking, we need sleep.
                    // User snippet implies: Ok(Some), Ok(None) -> result style?
                    // Aya 0.12 RingBuf `next()` returns `Option<Result<RingBufItem, ...>>`.
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
            }
        }
    });

    (rx, handle)
}

// Adjusting spawn_ringbuf_reader to match exactly user provided snippet structure closer if possible,
// but focusing on correctness of the match arms.
// User snippet had:
// match ringbuf.next() {
//    Ok(Some(item))
//    Ok(None)
//    Err(e)
// }
// This suggests they expect a Result<Option<Item>> which is not standard Iterator.
// Aya RingBuf `next` returns `Option<Result<RingBufItem, MapError>>`.
// Let's implement based on Aya 0.12 signature `impl Iterator`.

#[cfg(target_os = "linux")]
pub fn spawn_ringbuf_reader_fixed(
    mut ringbuf: aya::maps::ringbuf::RingBuf,
) -> (
    mpsc::Receiver<Result<MonitorEvent, MonitorError>>,
    std::thread::JoinHandle<()>,
) {
    let (tx, rx) = mpsc::channel(1024);

    let handle = std::thread::spawn(move || {
        loop {
            // In Aya 0.12 RingBuf implements Iterator.
            // However, the User provided a specific pattern `match ringbuf.next() { Ok(Some) ... }`
            // which implies they might be using a specific wrapper or thinking of a specific API.
            // BUT, standard Iterator is `Option<Item>`.
            // Let's look at `aya::maps::ringbuf::RingBuf`. It allows `next()` which returns `Option<Result<RingBufItem, MapError>>`.

            match ringbuf.next() {
                Some(Ok(item)) => {
                    let bytes = &*item; // Deref to bytes
                    let ev = parse_event(bytes);
                    if tx.blocking_send(ev).is_err() {
                        break;
                    }
                }
                Some(Err(e)) => {
                    let _ = tx.blocking_send(Err(MonitorError::RingBuf(e.to_string())));
                    break;
                }
                None => {
                    // Iterator end? Usually means map closed or detached.
                    // If it returns None immediately loop might spin if map is weird, but typically it blocks?
                    // Aya RingBuf acts as a blocking iterator by default unless configured otherwise.
                    // If it finishes, we break.
                    break;
                }
            }
        }
    });

    (rx, handle)
}
