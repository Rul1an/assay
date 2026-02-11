//! Resource limits and bounded readers for bundle verification.
//!
//! EINTR retry and byte limits for DoS protection.

use serde::Deserialize;
use std::io::{BufRead, Read};

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
            max_bundle_bytes: 100 * 1024 * 1024,  // 100 MB compressed
            max_decode_bytes: 1024 * 1024 * 1024, // 1 GB uncompressed
            max_manifest_bytes: 10 * 1024 * 1024, // 10 MB
            max_events_bytes: 500 * 1024 * 1024,  // 500 MB
            max_events: 100_000,
            max_line_bytes: 1024 * 1024, // 1 MB
            max_path_len: 256,
            max_json_depth: 64,
        }
    }
}

/// Partial overrides for `VerifyLimits`. Used for CLI/config JSON parsing.
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
        if self.read >= self.limit {
            return Err(std::io::Error::other(format!(
                "{}: exceeded limit of {} bytes",
                self.error_tag, self.limit
            )));
        }

        let max_to_read = (self.limit - self.read).min(buf.len() as u64) as usize;
        let n = self.inner.read(&mut buf[..max_to_read])?;
        self.read += n as u64;

        Ok(n)
    }
}

const MAX_EINTR_RETRIES: usize = 16;

/// A reader that transparently retries on EINTR.
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
                    if retries >= MAX_EINTR_RETRIES {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::Interrupted,
                            format!(
                                "persistent EINTR: interrupted {} consecutive times",
                                MAX_EINTR_RETRIES
                            ),
                        ));
                    }
                }
                other => return other,
            }
        }
    }
}

/// Read a line with a hard memory limit before allocation.
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
