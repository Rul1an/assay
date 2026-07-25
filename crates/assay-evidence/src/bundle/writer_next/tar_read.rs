use assay_common::limits::{LimitExceeded, LimitKind};
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
///
/// `max` is the maximum *payload* length in bytes; the trailing `\n` is not counted. A payload of
/// exactly `max` bytes is accepted, and `max + 1` is refused. This is the same interpretation
/// `check_events_shape` applies on the unverified path, so both paths share one budget for the
/// same input rather than each having its own accidental off-by-one.
///
/// Overflow is reported as a typed [`LimitExceeded`] cause inside an `io::Error`, so a caller
/// classifies via `LimitExceeded::from_io` and never by parsing the rendered message.
pub(crate) fn read_line_bounded<R: BufRead>(
    reader: &mut R,
    buf: &mut Vec<u8>,
    max: usize,
) -> std::io::Result<usize> {
    let overflow = || -> std::io::Error {
        std::io::Error::other(LimitExceeded {
            kind: LimitKind::LineBytes,
            limit: max as u64,
        })
    };
    let mut payload_len = 0usize;
    let mut total_read = 0usize;
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

                // Payload delta excludes the newline when we've found the end of the line, so a
                // line of exactly `max` payload bytes plus its `\n` is accepted.
                let payload_delta = if found { line_end - 1 } else { line_end };
                if payload_len + payload_delta > max {
                    return Err(overflow());
                }

                buf.extend_from_slice(&available[..line_end]);
                payload_len += payload_delta;
                (found, line_end)
            }
        };
        reader.consume(used);
        total_read += used;
        if done || total_read == 0 {
            return Ok(total_read);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::read_line_bounded;
    use assay_common::limits::{LimitExceeded, LimitKind};
    use std::io::{BufReader, Cursor};

    /// One interpretation, shared with `check_events_shape`: a line of exactly `max` payload bytes
    /// plus its `\n` is accepted, and `max + 1` payload bytes is refused. Before this alignment
    /// the two paths each had their own accidental interpretation and a caller could not tell
    /// which budget applied.
    #[test]
    fn a_payload_of_exactly_the_limit_plus_newline_is_accepted() {
        let payload = vec![b'x'; 100];
        let mut src = payload.clone();
        src.push(b'\n');
        let mut r = BufReader::new(Cursor::new(src));
        let mut buf = Vec::new();
        let n = read_line_bounded(&mut r, &mut buf, 100).expect("exact-limit payload must pass");
        assert_eq!(n, 101, "consumed bytes include the newline");
        assert_eq!(&buf, &{
            let mut e = payload.clone();
            e.push(b'\n');
            e
        });
    }

    #[test]
    fn one_payload_byte_over_the_limit_is_refused_with_a_typed_cause() {
        let mut src = vec![b'x'; 101];
        src.push(b'\n');
        let mut r = BufReader::new(Cursor::new(src));
        let mut buf = Vec::new();
        let err = read_line_bounded(&mut r, &mut buf, 100)
            .expect_err("101 payload bytes must be refused when the limit is 100");
        let cause =
            LimitExceeded::from_io(&err).expect("overflow must carry a typed cause, not a string");
        assert_eq!(cause.kind, LimitKind::LineBytes);
        assert_eq!(cause.limit, 100);
    }

    /// A line with no trailing newline (EOF-terminated) still counts payload as delivered bytes.
    /// The unterminated-overflow branch was the second string-only error site and had to move to
    /// the same typed cause.
    #[test]
    fn an_unterminated_line_over_the_limit_is_refused_with_a_typed_cause() {
        let src = vec![b'x'; 101];
        let mut r = BufReader::new(Cursor::new(src));
        let mut buf = Vec::new();
        let err = read_line_bounded(&mut r, &mut buf, 100)
            .expect_err("101 bytes with no newline must still be refused");
        let cause = LimitExceeded::from_io(&err)
            .expect("the unterminated overflow must also carry a typed cause");
        assert_eq!(cause.kind, LimitKind::LineBytes);
        assert_eq!(cause.limit, 100);
    }

    /// Acceptance twin for the previous case: a limit that fits must still accept the input, so
    /// the suite would not pass by simply refusing every unterminated line.
    #[test]
    fn an_unterminated_line_at_the_limit_is_accepted() {
        let src = vec![b'x'; 100];
        let mut r = BufReader::new(Cursor::new(src));
        let mut buf = Vec::new();
        let n =
            read_line_bounded(&mut r, &mut buf, 100).expect("100 bytes with no newline must pass");
        assert_eq!(n, 100);
        assert_eq!(buf.len(), 100);
    }
}
