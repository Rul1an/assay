use std::io::{BufRead, Read};

/// Many systems can deliver spurious interrupts during `read()`.
/// Retry only `Interrupted` for a bounded number of attempts.
const EINTR_RETRY_LIMIT: usize = 16;

pub(crate) struct EintrReader<R> {
    inner: R,
}

impl<R: Read> EintrReader<R> {
    pub(crate) fn new(inner: R) -> Self {
        Self { inner }
    }
}

impl<R: Read> Read for EintrReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut retries = 0;
        loop {
            match self.inner.read(buf) {
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {
                    retries += 1;
                    if retries >= EINTR_RETRY_LIMIT {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::Interrupted,
                            format!(
                                "persistent EINTR: interrupted {} consecutive times",
                                EINTR_RETRY_LIMIT
                            ),
                        ));
                    }
                }
                other => return other,
            }
        }
    }
}

/// Read a line with a hard memory limit before allocation growth.
pub(crate) fn read_line_bounded<R: BufRead>(
    reader: &mut R,
    buf: &mut Vec<u8>,
    max: usize,
) -> std::io::Result<usize> {
    let mut total_read = 0;
    loop {
        let (done, used) = {
            let available = reader.fill_buf()?;
            if available.is_empty() {
                (true, 0)
            } else {
                let (found, line_end) = match available.iter().position(|&b| b == b'\n') {
                    Some(pos) => (true, pos + 1),
                    None => (false, available.len()),
                };

                if total_read + line_end > max {
                    return Err(std::io::Error::other("LimitLineBytes: line exceeded limit"));
                }

                buf.extend_from_slice(&available[..line_end]);
                (found, line_end)
            }
        };
        reader.consume(used);
        total_read += used;
        if done || total_read == 0 {
            return Ok(total_read);
        }
        if total_read >= max && !done {
            return Err(std::io::Error::other("LimitLineBytes: line exceeded limit"));
        }
    }
}
