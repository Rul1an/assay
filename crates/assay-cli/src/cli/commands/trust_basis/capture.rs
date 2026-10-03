//! Byte capture for the one bundle reader that generation consumes.
//!
//! The sidecar's bundle identity must describe the bytes generation actually read,
//! not a second read of the same path. [`CaptureRead`] wraps the reader that is
//! already open, hashes and counts exactly the bytes each successful `read` returns,
//! and passes every error through unchanged.

use super::inputs::ByteIdentityV0;
use sha2::{Digest, Sha256};
use std::io::{self, Read};

pub(super) struct CaptureRead<R> {
    inner: R,
    hasher: Sha256,
    bytes: u64,
    at_eof: bool,
}

impl<R> CaptureRead<R> {
    pub(super) fn new(inner: R) -> Self {
        Self {
            inner,
            hasher: Sha256::new(),
            bytes: 0,
            at_eof: false,
        }
    }

    /// The identity of every byte returned so far, or `None` unless the last read
    /// was end of file. Only a zero-byte answer to a non-empty buffer is end of
    /// file; a read into an empty buffer returns zero without proving anything.
    pub(super) fn finish(self) -> Option<ByteIdentityV0> {
        self.at_eof.then(|| ByteIdentityV0 {
            sha256: hex::encode(self.hasher.finalize()),
            bytes: self.bytes,
        })
    }
}

impl<R: Read> Read for CaptureRead<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let read = self.inner.read(buf)?;
        if read == 0 {
            if !buf.is_empty() {
                self.at_eof = true;
            }
        } else {
            self.hasher.update(&buf[..read]);
            self.bytes += read as u64;
            self.at_eof = false;
        }
        Ok(read)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::collections::VecDeque;
    use std::io::{Error, ErrorKind};
    use std::rc::Rc;

    /// FIPS 180-2 vectors, written out here so the capture is checked against a
    /// value no production helper computed.
    const ABC_SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    /// Serves a scripted sequence of answers, one per `read` call, and counts calls.
    struct Scripted {
        answers: VecDeque<io::Result<Vec<u8>>>,
        calls: Rc<Cell<usize>>,
    }

    impl Scripted {
        fn new(answers: Vec<io::Result<&[u8]>>) -> Self {
            Self {
                answers: answers
                    .into_iter()
                    .map(|answer| answer.map(<[u8]>::to_vec))
                    .collect(),
                calls: Rc::new(Cell::new(0)),
            }
        }
    }

    impl Read for Scripted {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.calls.set(self.calls.get() + 1);
            match self.answers.pop_front() {
                Some(Ok(bytes)) => {
                    assert!(bytes.len() <= buf.len(), "script answer larger than buffer");
                    buf[..bytes.len()].copy_from_slice(&bytes);
                    Ok(bytes.len())
                }
                Some(Err(error)) => Err(error),
                None => Ok(0),
            }
        }
    }

    #[test]
    fn short_reads_are_hashed_in_order_and_end_of_file_completes_the_identity() {
        let mut capture = CaptureRead::new(Scripted::new(vec![Ok(b"a"), Ok(b"bc"), Ok(b"")]));
        let mut out = Vec::new();
        capture.read_to_end(&mut out).unwrap();
        assert_eq!(out, b"abc");
        let identity = capture.finish().expect("end of file was observed");
        assert_eq!(identity.sha256, ABC_SHA256);
        assert_eq!(identity.bytes, 3);
    }

    #[test]
    fn an_empty_input_has_the_empty_digest_once_end_of_file_is_seen() {
        let mut capture = CaptureRead::new(Scripted::new(vec![]));
        assert_eq!(capture.read(&mut [0u8; 8]).unwrap(), 0);
        let identity = capture.finish().expect("end of file was observed");
        assert_eq!(identity.sha256, EMPTY_SHA256);
        assert_eq!(identity.bytes, 0);
    }

    #[test]
    fn a_zero_length_read_does_not_prove_end_of_file() {
        let mut capture = CaptureRead::new(Scripted::new(vec![Ok(b"abc")]));
        assert_eq!(capture.read(&mut [0u8; 8]).unwrap(), 3);
        assert_eq!(capture.read(&mut []).unwrap(), 0);
        assert!(capture.finish().is_none());
    }

    #[test]
    fn stopping_before_end_of_file_yields_no_identity_and_reads_nothing_more() {
        let scripted = Scripted::new(vec![Ok(b"ab"), Ok(b"c")]);
        let calls = Rc::clone(&scripted.calls);
        let mut capture = CaptureRead::new(scripted);
        assert_eq!(capture.read(&mut [0u8; 8]).unwrap(), 2);
        // `finish` reports the incomplete read; it never drains the rest to make one.
        assert!(capture.finish().is_none());
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn errors_pass_through_unchanged_and_count_no_bytes() {
        let mut capture = CaptureRead::new(Scripted::new(vec![
            Ok(b"ab"),
            Err(Error::new(ErrorKind::Interrupted, "interrupted")),
            Err(Error::other("device gone")),
            Ok(b"c"),
        ]));
        let mut buf = [0u8; 8];
        assert_eq!(capture.read(&mut buf).unwrap(), 2);
        assert_eq!(
            capture.read(&mut buf).unwrap_err().kind(),
            ErrorKind::Interrupted
        );
        let error = capture.read(&mut buf).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Other);
        assert_eq!(error.to_string(), "device gone");
        assert_eq!(capture.read(&mut buf).unwrap(), 1);
        assert_eq!(capture.read(&mut buf).unwrap(), 0);
        let identity = capture.finish().unwrap();
        assert_eq!(
            identity.sha256, ABC_SHA256,
            "only returned bytes are hashed"
        );
        assert_eq!(identity.bytes, 3);
    }

    #[test]
    fn bytes_after_an_end_of_file_answer_reopen_the_identity() {
        let mut capture = CaptureRead::new(Scripted::new(vec![Ok(b"ab"), Ok(b""), Ok(b"c")]));
        let mut buf = [0u8; 8];
        assert_eq!(capture.read(&mut buf).unwrap(), 2);
        assert_eq!(capture.read(&mut buf).unwrap(), 0);
        assert_eq!(capture.read(&mut buf).unwrap(), 1);
        assert!(
            capture.finish().is_none(),
            "the last answer was data, not end of file"
        );
    }

    #[test]
    fn read_to_end_through_take_records_every_byte_the_caller_received() {
        // Generation reads through `Take`; the capture sits under it and sees only
        // what was actually returned.
        let mut capture = CaptureRead::new(Scripted::new(vec![Ok(b"abc")]));
        let mut out = Vec::new();
        (&mut capture).take(1024).read_to_end(&mut out).unwrap();
        assert_eq!(capture.finish().unwrap().sha256, ABC_SHA256);
    }
}
