//! Bounded ingest primitive shared by every verifier that reads untrusted archive bytes.
//!
//! ADR-043 §1 states the invariant: a verifier entry point applies its byte ceiling to the
//! stream *before* the input is materialized. Reading an untrusted artifact into memory ahead
//! of the limit check is a contract defect regardless of what the subsequent verification
//! concludes.
//!
//! Only the mechanism lives here. Each crate keeps its own limit vocabulary, because those
//! carry domain meaning that does not travel: `max_manifest_bytes` and `max_events` describe an
//! evidence bundle and say nothing about a replay bundle. One mechanism, several vocabularies.

#[cfg(feature = "std")]
use std::io::Read;

/// A reader that refuses to yield more than `limit` bytes from the wrapped stream.
///
/// The limit is **inclusive**: an input of exactly `limit` bytes is accepted and `limit + 1` is
/// refused. That boundary is load-bearing and easy to get wrong. Erroring as soon as
/// `read == limit` rejects the exact-limit input, because every consumer issues one further read
/// to observe EOF, which silently makes the effective ceiling `limit - 1`. At the boundary this
/// reader therefore probes a single byte: EOF means the input fit, one byte means it did not.
///
/// `error_tag` names the ceiling that was crossed so the failure identifies which limit applied
/// rather than reporting a generic truncation.
#[cfg(feature = "std")]
pub struct LimitReader<R> {
    inner: R,
    limit: u64,
    read: u64,
    error_tag: &'static str,
}

#[cfg(feature = "std")]
impl<R: Read> LimitReader<R> {
    pub fn new(inner: R, limit: u64, error_tag: &'static str) -> Self {
        Self {
            inner,
            limit,
            read: 0,
            error_tag,
        }
    }

    /// Bytes yielded so far. Useful to a caller that wants to report how much of the ceiling a
    /// well-formed input actually consumed.
    pub fn bytes_read(&self) -> u64 {
        self.read
    }
}

#[cfg(feature = "std")]
impl<R: Read> Read for LimitReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.read >= self.limit {
            let mut probe = [0u8; 1];
            return match self.inner.read(&mut probe)? {
                0 => Ok(0),
                _ => Err(std::io::Error::other(std::format!(
                    "{}: exceeded limit of {} bytes",
                    self.error_tag,
                    self.limit
                ))),
            };
        }

        let max_to_read = (self.limit - self.read).min(buf.len() as u64) as usize;
        let n = self.inner.read(&mut buf[..max_to_read])?;
        self.read += n as u64;

        Ok(n)
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::LimitReader;
    use std::io::{Cursor, Read};
    use std::string::ToString;
    use std::vec;
    use std::vec::Vec;

    fn read_all(len: usize, limit: u64) -> std::io::Result<usize> {
        let mut r = LimitReader::new(Cursor::new(vec![0u8; len]), limit, "T");
        let mut out = Vec::new();
        r.read_to_end(&mut out).map(|_| out.len())
    }

    /// The boundary the ADR-043 acceptance bar names. Before the EOF probe existed, the exact
    /// limit was refused and the effective ceiling was one byte lower than the configured one.
    #[test]
    fn exact_limit_is_accepted_and_one_more_byte_is_refused() {
        assert_eq!(read_all(99, 100).unwrap(), 99);
        assert_eq!(
            read_all(100, 100).unwrap(),
            100,
            "an input of exactly the limit must be accepted"
        );
        assert!(read_all(101, 100).is_err(), "limit + 1 must be refused");
    }

    #[test]
    fn a_zero_limit_accepts_only_empty_input() {
        assert_eq!(read_all(0, 0).unwrap(), 0);
        assert!(read_all(1, 0).is_err());
    }

    /// A stream that never fills the caller's buffer must not be able to walk past the ceiling
    /// one byte at a time.
    #[test]
    fn short_reads_cannot_walk_past_the_limit() {
        struct OneByteAtATime(Cursor<Vec<u8>>);
        impl Read for OneByteAtATime {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                if buf.is_empty() {
                    return Ok(0);
                }
                self.0.read(&mut buf[..1])
            }
        }

        let mut over = LimitReader::new(OneByteAtATime(Cursor::new(vec![0u8; 101])), 100, "T");
        let mut out = Vec::new();
        assert!(over.read_to_end(&mut out).is_err());

        let mut exact = LimitReader::new(OneByteAtATime(Cursor::new(vec![0u8; 100])), 100, "T");
        let mut out = Vec::new();
        assert_eq!(exact.read_to_end(&mut out).unwrap(), 100);
    }

    /// The tag identifies which ceiling was crossed, so a failure says whether the compressed
    /// input, the decoded stream or a single member was too large.
    #[test]
    fn the_error_names_the_ceiling_that_was_crossed() {
        let err = read_all(101, 100).unwrap_err();
        assert!(err.to_string().contains('T'), "{err}");
        assert!(err.to_string().contains("100"), "{err}");
    }

    #[test]
    fn bytes_read_reports_consumption() {
        let mut r = LimitReader::new(Cursor::new(vec![0u8; 40]), 100, "T");
        let mut out = Vec::new();
        r.read_to_end(&mut out).unwrap();
        assert_eq!(r.bytes_read(), 40);
    }
}
