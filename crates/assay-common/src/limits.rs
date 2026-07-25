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

/// A ceiling was crossed. Carried as data rather than as a formatted message, so a caller
/// classifies by inspecting fields and never by parsing text.
///
/// `kind` is opaque here on purpose. This crate owns the mechanism, not the vocabulary: the
/// evidence verifier and the replay verifier name their ceilings differently and each maps this
/// back to its own error code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LimitExceeded {
    pub kind: &'static str,
    pub limit: u64,
}

impl core::fmt::Display for LimitExceeded {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}: exceeded limit of {} bytes", self.kind, self.limit)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for LimitExceeded {}

#[cfg(feature = "std")]
impl LimitExceeded {
    /// Recover the typed cause from an `io::Error` produced by a [`LimitReader`], or `None` if
    /// the failure came from somewhere else. This is the supported way to classify: matching on
    /// the rendered message is not a contract.
    pub fn from_io(err: &std::io::Error) -> Option<Self> {
        err.get_ref()
            .and_then(|inner| inner.downcast_ref::<LimitExceeded>())
            .copied()
    }
}

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
    tripped: bool,
    error_tag: &'static str,
}

#[cfg(feature = "std")]
impl<R: Read> LimitReader<R> {
    pub fn new(inner: R, limit: u64, error_tag: &'static str) -> Self {
        Self {
            inner,
            limit,
            read: 0,
            tripped: false,
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
#[cfg(feature = "std")]
impl<R> LimitReader<R> {
    fn overflow(&self) -> std::io::Error {
        std::io::Error::other(LimitExceeded {
            kind: self.error_tag,
            limit: self.limit,
        })
    }
}

#[cfg(feature = "std")]
impl<R: Read> Read for LimitReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        // The overflow is sticky. A consumer that swallows the first error and reads again must
        // not then be handed a clean EOF, which would present a truncated archive as a complete
        // one. Once the ceiling is crossed this reader only ever errors.
        if self.tripped {
            return Err(self.overflow());
        }

        // `Read` requires that an empty buffer yields `Ok(0)` without consuming the source. The
        // boundary probe below reads a byte, so it must not run for a zero-length request.
        if buf.is_empty() {
            return Ok(0);
        }

        if self.read >= self.limit {
            let mut probe = [0u8; 1];
            return match self.inner.read(&mut probe)? {
                0 => Ok(0),
                _ => {
                    self.tripped = true;
                    Err(self.overflow())
                }
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

    /// A consumer that ignores the overflow and reads again must not be handed EOF. Without the
    /// sticky flag the retry probes an exhausted source, sees 0, and reports a clean end of
    /// input, which presents a truncated archive as a complete one.
    #[test]
    fn the_overflow_is_sticky_and_never_degrades_into_eof() {
        let mut r = LimitReader::new(Cursor::new(vec![0u8; 101]), 100, "T");
        let mut sink = [0u8; 256];
        // Drain to the ceiling, then trip it.
        let mut tripped = false;
        for _ in 0..8 {
            match r.read(&mut sink) {
                Ok(0) => panic!("must not report EOF for an oversized source"),
                Ok(_) => continue,
                Err(_) => {
                    tripped = true;
                    break;
                }
            }
        }
        assert!(
            tripped,
            "the ceiling must be crossed by an oversized source"
        );
        for _ in 0..3 {
            assert!(
                r.read(&mut sink).is_err(),
                "every subsequent read must keep failing, never return EOF"
            );
        }
    }

    /// `Read` requires an empty buffer to yield `Ok(0)` without consuming. The boundary probe
    /// reads a byte, so it must not run for a zero-length request.
    #[test]
    fn an_empty_read_consumes_nothing() {
        let mut r = LimitReader::new(Cursor::new(vec![0u8; 10]), 4, "T");
        assert_eq!(r.read(&mut []).unwrap(), 0);
        assert_eq!(
            r.bytes_read(),
            0,
            "an empty read must not consume the source"
        );

        // And at the boundary, where the probe would otherwise fire.
        let mut out = [0u8; 4];
        let _ = r.read(&mut out).unwrap();
        assert_eq!(r.read(&mut []).unwrap(), 0);

        // The source still has bytes left, so a real read must still detect the overflow.
        assert!(r.read(&mut out).is_err());
    }

    /// The cause travels as data. A caller must be able to recover `kind` and `limit` without
    /// looking at the rendered message, because message text is not a contract.
    #[test]
    fn the_cause_is_recoverable_as_a_typed_value() {
        let err = read_all(101, 100).unwrap_err();
        let cause =
            super::LimitExceeded::from_io(&err).expect("typed cause must survive the io layer");
        assert_eq!(cause.kind, "T");
        assert_eq!(cause.limit, 100);
    }

    #[test]
    fn an_unrelated_io_error_is_not_mistaken_for_a_ceiling() {
        let other = std::io::Error::other("something else entirely");
        assert!(super::LimitExceeded::from_io(&other).is_none());
    }

    #[test]
    fn bytes_read_reports_consumption() {
        let mut r = LimitReader::new(Cursor::new(vec![0u8; 40]), 100, "T");
        let mut out = Vec::new();
        r.read_to_end(&mut out).unwrap();
        assert_eq!(r.bytes_read(), 40);
    }
}
