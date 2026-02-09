use std::io::{Cursor, Read};

use sha2::{Digest, Sha256};

use crate::canonicalize::{compute_canonical_digest, CanonicalizeError};

pub(crate) fn sha256_hex_reader<R: Read>(mut reader: R) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buf = [0_u8; 8192];

    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }

    Ok(format!("sha256:{:x}", hasher.finalize()))
}

pub(crate) fn sha256_hex_bytes(bytes: &[u8]) -> String {
    // In-memory hashing should be infallible; keep a single hashing implementation.
    sha256_hex_reader(Cursor::new(bytes)).expect("hashing in-memory bytes via cursor must not fail")
}

pub(crate) fn compute_canonical_or_raw_digest<F>(content: &str, on_canonical_error: F) -> String
where
    F: FnOnce(&CanonicalizeError),
{
    match compute_canonical_digest(content) {
        Ok(digest) => digest,
        Err(e) => {
            on_canonical_error(&e);
            sha256_hex_bytes(content.as_bytes())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    struct ChunkedReader<'a> {
        data: &'a [u8],
        pos: usize,
        max_chunk: usize,
    }

    impl<'a> Read for ChunkedReader<'a> {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            if self.pos >= self.data.len() {
                return Ok(0);
            }
            let n = out
                .len()
                .min(self.max_chunk)
                .min(self.data.len().saturating_sub(self.pos));
            out[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
            self.pos += n;
            Ok(n)
        }
    }

    #[test]
    fn sha256_reader_matches_bytes_digest() {
        let payload = b"\x00\x01hello\xffbinary\n";
        let from_bytes = sha256_hex_bytes(payload);
        let from_reader = sha256_hex_reader(Cursor::new(payload)).expect("reader hashing");
        assert_eq!(from_bytes, from_reader);
    }

    #[test]
    fn sha256_reader_chunked_stream_parity() {
        let payload = b"abcdefghijklmnopqrstuvwxyz0123456789";
        let from_bytes = sha256_hex_bytes(payload);
        let chunked = ChunkedReader {
            data: payload,
            pos: 0,
            max_chunk: 3,
        };
        let from_chunked = sha256_hex_reader(chunked).expect("chunked reader hashing");
        assert_eq!(from_bytes, from_chunked);
    }
}
