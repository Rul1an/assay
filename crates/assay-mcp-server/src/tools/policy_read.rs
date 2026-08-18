//! One bounded policy-byte reader. The inclusive ceiling lives in
//! `assay_common::limits::LimitReader`; this module owns accumulation and the
//! server mapping onto that primitive.

use super::{ToolContext, ToolError};
use assay_common::limits::{LimitExceeded, LimitKind, LimitReader};
use std::fs::File;
use std::io::{self, Read};

/// Read at most `limit` bytes from an arbitrary `Read`. Metadata is not consulted.
pub(super) fn read_bounded<R: Read>(reader: R, limit: usize) -> io::Result<Vec<u8>> {
    let mut reader = LimitReader::new(reader, limit as u64, LimitKind::SourceBytes);
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn map_read_error(err: io::Error, rel_path: &str) -> ToolError {
    if err.kind() == io::ErrorKind::NotFound {
        return ToolError::new(
            "E_POLICY_NOT_FOUND",
            &format!("Policy not found: {rel_path}"),
        );
    }
    if LimitExceeded::from_io(&err).is_some() {
        return ToolError::new(
            "E_LIMIT_EXCEEDED",
            "policy file exceeds the configured byte limit",
        );
    }
    ToolError::new("E_POLICY_READ", &err.to_string())
}

impl ToolContext {
    /// Resolve `user_path`, then read the file through `read_bounded` on a
    /// blocking thread. The `File` is opened inside `spawn_blocking`.
    pub async fn read_policy_bounded(&self, user_path: &str) -> Result<Vec<u8>, ToolError> {
        self.read_policy_bounded_with_limit(user_path, crate::config::policy_byte_limit_from_env())
            .await
    }

    /// Same ingest path as production, with an explicit ceiling for focused tests.
    pub(crate) async fn read_policy_bounded_with_limit(
        &self,
        user_path: &str,
        limit: usize,
    ) -> Result<Vec<u8>, ToolError> {
        let path = self.resolve_policy_path(user_path).await?;
        let rel_path = user_path.to_string();
        match tokio::task::spawn_blocking(move || {
            let file = File::open(&path)?;
            read_bounded(file, limit)
        })
        .await
        {
            Ok(Ok(bytes)) => Ok(bytes),
            Ok(Err(err)) => Err(map_read_error(err, &rel_path)),
            Err(join) => Err(ToolError::new("E_POLICY_READ", &join.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::PolicyCaches;
    use crate::config::ServerConfig;
    use crate::tools::ToolContext;
    use std::io::{self, Read, Seek, SeekFrom, Write};
    use std::path::PathBuf;
    use tempfile::TempDir;

    struct OneByteAtATime<R>(R);

    impl<R: Read> Read for OneByteAtATime<R> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if buf.is_empty() {
                return Ok(0);
            }
            self.0.read(&mut buf[..1])
        }
    }

    struct GrowingReader {
        first: Vec<u8>,
        rest: Vec<u8>,
        stage: u8,
    }

    impl Read for GrowingReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            match self.stage {
                0 => {
                    self.stage = 1;
                    let n = self.first.len().min(buf.len());
                    buf[..n].copy_from_slice(&self.first[..n]);
                    Ok(n)
                }
                1 => {
                    self.stage = 2;
                    let n = self.rest.len().min(buf.len());
                    buf[..n].copy_from_slice(&self.rest[..n]);
                    Ok(n)
                }
                _ => Ok(0),
            }
        }
    }

    struct FailingReader;

    impl Read for FailingReader {
        fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("injected read failure"))
        }
    }

    fn classify(err: &io::Error) -> Option<LimitExceeded> {
        LimitExceeded::from_io(err)
    }

    #[test]
    fn read_bounded_accepts_exact_limit_and_refuses_one_more() {
        let exact = read_bounded(io::Cursor::new(vec![b'a'; 8]), 8).expect("exact limit");
        assert_eq!(exact.len(), 8);
        let err = read_bounded(io::Cursor::new(vec![b'a'; 9]), 8).expect_err("limit+1");
        let cause = classify(&err).expect("typed limit cause");
        assert_eq!(cause.kind, LimitKind::SourceBytes);
        assert_eq!(cause.limit, 8);
    }

    #[test]
    fn read_bounded_chunked_crossing_trips_the_same_rule() {
        let err = read_bounded(OneByteAtATime(io::Cursor::new(vec![b'x'; 5])), 4)
            .expect_err("chunked limit+1");
        assert!(
            classify(&err).is_some(),
            "must classify via LimitExceeded::from_io"
        );
        let exact =
            read_bounded(OneByteAtATime(io::Cursor::new(vec![b'x'; 4])), 4).expect("chunked exact");
        assert_eq!(exact.len(), 4);
    }

    #[test]
    fn read_bounded_growing_reader_trips_after_the_first_chunk() {
        let err = read_bounded(
            GrowingReader {
                first: vec![b'a'; 4],
                rest: vec![b'b'; 2],
                stage: 0,
            },
            4,
        )
        .expect_err("growth past limit");
        assert!(classify(&err).is_some(), "growth must be a typed limit");
    }

    #[test]
    fn read_bounded_ordinary_io_is_not_a_limit() {
        let err = read_bounded(FailingReader, 16).expect_err("injected io");
        assert!(
            classify(&err).is_none(),
            "ordinary I/O must not look like LimitExceeded"
        );
        assert!(err.to_string().contains("injected read failure"));
    }

    async fn context_at(root: PathBuf) -> ToolContext {
        let canon = tokio::fs::canonicalize(&root).await.unwrap();
        ToolContext {
            policy_root: root,
            policy_root_canon: canon,
            cfg: ServerConfig::default(),
            caches: PolicyCaches::new(8),
        }
    }

    #[tokio::test]
    async fn async_entry_maps_missing_file() {
        let tmp = TempDir::new().unwrap();
        let ctx = context_at(tmp.path().to_path_buf()).await;
        let err = ctx
            .read_policy_bounded_with_limit("missing.yaml", 32)
            .await
            .expect_err("missing");
        assert_eq!(err.code, "E_POLICY_NOT_FOUND");
        assert_eq!(err.message, "Policy not found: missing.yaml");
        assert!(
            !err.message.contains(tmp.path().to_string_lossy().as_ref()),
            "must not publish the canonical root"
        );
    }

    #[tokio::test]
    async fn async_entry_maps_directory_read_as_other_io() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir(tmp.path().join("not-a-file")).unwrap();
        let ctx = context_at(tmp.path().to_path_buf()).await;
        let err = ctx
            .read_policy_bounded_with_limit("not-a-file", 32)
            .await
            .expect_err("directory");
        assert_eq!(err.code, "E_POLICY_READ");
        assert!(
            !err.message.is_empty(),
            "other I/O keeps the current diagnostic text"
        );
    }

    #[tokio::test]
    async fn async_entry_accepts_exact_limit_file() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("exact.yaml"), vec![b'e'; 16]).unwrap();
        let ctx = context_at(tmp.path().to_path_buf()).await;
        let bytes = ctx
            .read_policy_bounded_with_limit("exact.yaml", 16)
            .await
            .unwrap_or_else(|err| {
                panic!("exact file must be accepted: {} {}", err.code, err.message)
            });
        assert_eq!(bytes.len(), 16);
    }

    #[tokio::test]
    async fn async_entry_refuses_sparse_limit_plus_one() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("sparse.yaml");
        let mut file = File::create(&path).unwrap();
        file.seek(SeekFrom::Start(16)).unwrap();
        file.write_all(&[0u8]).unwrap();
        file.sync_all().unwrap();
        drop(file);

        let ctx = context_at(tmp.path().to_path_buf()).await;
        let err = ctx
            .read_policy_bounded_with_limit("sparse.yaml", 16)
            .await
            .expect_err("sparse limit+1");
        assert_eq!(err.code, "E_LIMIT_EXCEEDED");
        assert_eq!(err.message, "policy file exceeds the configured byte limit");
    }
}
