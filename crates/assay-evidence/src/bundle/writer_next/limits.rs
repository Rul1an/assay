use serde::Deserialize;
use std::io::Read;

/// Resource limits for bundle verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerifyLimits {
    pub max_bundle_bytes: u64,
    pub max_decode_bytes: u64,
    pub max_manifest_bytes: u64,
    pub max_events_bytes: u64,
    pub max_events: usize,
    pub max_line_bytes: usize,
    pub max_path_len: usize,
    pub max_json_depth: usize,
}

impl Default for VerifyLimits {
    fn default() -> Self {
        Self {
            max_bundle_bytes: 100_u64 * 1024 * 1024,
            max_decode_bytes: 1024_u64 * 1024 * 1024,
            max_manifest_bytes: 10_u64 * 1024 * 1024,
            max_events_bytes: 500_u64 * 1024 * 1024,
            max_events: 100_000,
            max_line_bytes: 1024 * 1024,
            max_path_len: 256,
            max_json_depth: 64,
        }
    }
}

/// Partial overrides for `VerifyLimits`. Used for CLI/config JSON parsing.
/// Unknown keys cause deserialization to fail (deny_unknown_fields).
/// Merge with `VerifyLimits::default().apply(overrides)`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifyLimitsOverrides {
    pub max_bundle_bytes: Option<u64>,
    pub max_decode_bytes: Option<u64>,
    pub max_manifest_bytes: Option<u64>,
    pub max_events_bytes: Option<u64>,
    pub max_events: Option<usize>,
    pub max_line_bytes: Option<usize>,
    pub max_path_len: Option<usize>,
    pub max_json_depth: Option<usize>,
}

impl VerifyLimits {
    /// Apply overrides onto these defaults. Only `Some` values override.
    pub fn apply(self, overrides: VerifyLimitsOverrides) -> Self {
        Self {
            max_bundle_bytes: overrides.max_bundle_bytes.unwrap_or(self.max_bundle_bytes),
            max_decode_bytes: overrides.max_decode_bytes.unwrap_or(self.max_decode_bytes),
            max_manifest_bytes: overrides
                .max_manifest_bytes
                .unwrap_or(self.max_manifest_bytes),
            max_events_bytes: overrides.max_events_bytes.unwrap_or(self.max_events_bytes),
            max_events: overrides.max_events.unwrap_or(self.max_events),
            max_line_bytes: overrides.max_line_bytes.unwrap_or(self.max_line_bytes),
            max_path_len: overrides.max_path_len.unwrap_or(self.max_path_len),
            max_json_depth: overrides.max_json_depth.unwrap_or(self.max_json_depth),
        }
    }
}

/// A reader that limits the total number of bytes read and fails explicitly on overflow.
pub(crate) struct LimitReader<R> {
    inner: R,
    limit: u64,
    read: u64,
    error_tag: &'static str,
}

impl<R: Read> LimitReader<R> {
    pub(crate) fn new(inner: R, limit: u64, error_tag: &'static str) -> Self {
        Self {
            inner,
            limit,
            read: 0,
            error_tag,
        }
    }
}

impl<R: Read> Read for LimitReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        // The limit is inclusive: an input of exactly `limit` bytes is accepted, `limit + 1` is
        // refused. Erroring as soon as `read == limit` rejects the exact-limit input, because
        // every consumer issues one further read to observe EOF, which silently made the
        // effective ceiling `limit - 1`. At the boundary we therefore probe a single byte: EOF
        // means the input fit, one byte means it did not.
        if self.read >= self.limit {
            let mut probe = [0u8; 1];
            return match self.inner.read(&mut probe)? {
                0 => Ok(0),
                _ => Err(std::io::Error::other(format!(
                    "{}: exceeded limit of {} bytes",
                    self.error_tag, self.limit
                ))),
            };
        }

        let max_to_read = (self.limit - self.read).min(buf.len() as u64) as usize;
        let n = self.inner.read(&mut buf[..max_to_read])?;
        self.read += n as u64;

        Ok(n)
    }
}

#[cfg(test)]
mod limit_reader_boundary_tests {
    use super::LimitReader;
    use std::io::{Cursor, Read};

    fn read_all(len: usize, limit: u64) -> std::io::Result<usize> {
        let mut r = LimitReader::new(Cursor::new(vec![0u8; len]), limit, "T");
        let mut out = Vec::new();
        r.read_to_end(&mut out).map(|_| out.len())
    }

    /// The boundary ADR-043 §1 specifies. Before the EOF probe, `read == limit` errored on the
    /// EOF-detecting read every consumer makes, so the effective ceiling was `limit - 1`.
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

    /// A stream that never fills the caller's buffer must not walk past the ceiling one byte at
    /// a time.
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
        assert!(over.read_to_end(&mut Vec::new()).is_err());

        let mut exact = LimitReader::new(OneByteAtATime(Cursor::new(vec![0u8; 100])), 100, "T");
        assert_eq!(exact.read_to_end(&mut Vec::new()).unwrap(), 100);
    }

    #[test]
    fn the_error_names_the_ceiling_that_was_crossed() {
        let err = read_all(101, 100).unwrap_err();
        assert!(err.to_string().contains('T'), "{err}");
        assert!(err.to_string().contains("100"), "{err}");
    }
}
