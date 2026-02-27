//! Host-side AttachmentWriter implementations for protocol adapter payload preservation.

use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use assay_adapter_api::{
    AdapterError, AdapterErrorKind, AdapterResult, AttachmentWriter, RawPayloadRef,
};
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;

/// Host-enforced policy for preserving adapter raw payloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentWritePolicy {
    /// Hard size ceiling applied before persistence.
    pub max_payload_bytes: u64,
    /// Explicit allowlist of canonical media types accepted by the host.
    pub allowed_media_types: BTreeSet<String>,
}

impl AttachmentWritePolicy {
    /// Create a new attachment policy.
    #[must_use]
    pub fn new<I, S>(max_payload_bytes: u64, allowed_media_types: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            max_payload_bytes,
            allowed_media_types: allowed_media_types.into_iter().map(Into::into).collect(),
        }
    }

    fn validate(&self, payload: &[u8], media_type: &str) -> AdapterResult<String> {
        if payload.len() as u64 > self.max_payload_bytes {
            return Err(AdapterError::new(
                AdapterErrorKind::Measurement,
                format!(
                    "payload exceeds attachment policy max_payload_bytes ({})",
                    self.max_payload_bytes
                ),
            ));
        }

        let canonical_media_type = canonicalize_media_type(media_type)?;
        if !self.allowed_media_types.contains(&canonical_media_type) {
            return Err(AdapterError::new(
                AdapterErrorKind::Measurement,
                format!("unsupported attachment media type: {canonical_media_type}"),
            ));
        }

        Ok(canonical_media_type)
    }
}

/// Filesystem-backed host AttachmentWriter with explicit policy enforcement.
#[derive(Debug, Clone)]
pub struct FilesystemAttachmentWriter {
    root: PathBuf,
    policy: AttachmentWritePolicy,
}

impl FilesystemAttachmentWriter {
    /// Create a new filesystem-backed attachment writer.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>, policy: AttachmentWritePolicy) -> Self {
        Self {
            root: root.into(),
            policy,
        }
    }

    /// Return the root directory used for persisted payloads.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Resolve the stored payload path for a digest.
    #[must_use]
    pub fn stored_path(&self, sha256: &str) -> PathBuf {
        let shard = &sha256[..2];
        self.root.join(shard).join(sha256)
    }
}

impl AttachmentWriter for FilesystemAttachmentWriter {
    fn write_raw_payload(&self, payload: &[u8], media_type: &str) -> AdapterResult<RawPayloadRef> {
        let canonical_media_type = self.policy.validate(payload, media_type)?;
        let sha256 = sha256_hex(payload);
        let target = self.stored_path(&sha256);

        if target.exists() {
            return Ok(RawPayloadRef {
                sha256,
                size_bytes: payload.len() as u64,
                media_type: canonical_media_type,
            });
        }

        let parent = target.parent().ok_or_else(|| {
            AdapterError::new(
                AdapterErrorKind::Infrastructure,
                "attachment target path has no parent directory",
            )
        })?;

        fs::create_dir_all(parent).map_err(|err| {
            AdapterError::new(
                AdapterErrorKind::Infrastructure,
                format!("failed to prepare attachment directory: {err}"),
            )
        })?;

        let mut temp = NamedTempFile::new_in(parent).map_err(|err| {
            AdapterError::new(
                AdapterErrorKind::Infrastructure,
                format!("failed to allocate attachment temp file: {err}"),
            )
        })?;

        temp.write_all(payload).map_err(|err| {
            AdapterError::new(
                AdapterErrorKind::Infrastructure,
                format!("failed to write attachment payload: {err}"),
            )
        })?;
        temp.flush().map_err(|err| {
            AdapterError::new(
                AdapterErrorKind::Infrastructure,
                format!("failed to flush attachment payload: {err}"),
            )
        })?;

        if let Err(err) = temp.persist(&target) {
            if !target.exists() {
                return Err(AdapterError::new(
                    AdapterErrorKind::Infrastructure,
                    format!("failed to persist attachment payload: {}", err.error),
                ));
            }
        }

        Ok(RawPayloadRef {
            sha256,
            size_bytes: payload.len() as u64,
            media_type: canonical_media_type,
        })
    }
}

fn canonicalize_media_type(media_type: &str) -> AdapterResult<String> {
    let canonical = media_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();

    if canonical.is_empty() {
        return Err(AdapterError::new(
            AdapterErrorKind::Measurement,
            "attachment media type must not be empty",
        ));
    }

    let valid = canonical
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'+' | b'-'));
    if !valid || !canonical.contains('/') {
        return Err(AdapterError::new(
            AdapterErrorKind::Measurement,
            "attachment media type is invalid",
        ));
    }

    Ok(canonical)
}

fn sha256_hex(payload: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(payload);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> AttachmentWritePolicy {
        AttachmentWritePolicy::new(1024, ["application/json", "application/octet-stream"])
    }

    #[test]
    fn attachment_writer_persists_payload_and_returns_digest_ref() {
        let dir = tempfile::tempdir().unwrap();
        let writer = FilesystemAttachmentWriter::new(dir.path(), policy());
        let payload = br#"{"hello":"world"}"#;

        let raw_ref = writer
            .write_raw_payload(payload, "Application/JSON; charset=utf-8")
            .unwrap();

        assert_eq!(raw_ref.size_bytes, payload.len() as u64);
        assert_eq!(raw_ref.media_type, "application/json");
        assert_eq!(raw_ref.sha256, sha256_hex(payload));
        assert_eq!(
            fs::read(writer.stored_path(&raw_ref.sha256)).unwrap(),
            payload
        );
    }

    #[test]
    fn attachment_writer_rejects_oversize_payload_as_measurement() {
        let dir = tempfile::tempdir().unwrap();
        let writer = FilesystemAttachmentWriter::new(
            dir.path(),
            AttachmentWritePolicy::new(4, ["application/json"]),
        );
        let payload = br#"{"super":"secret-token"}"#;

        let err = writer
            .write_raw_payload(payload, "application/json")
            .unwrap_err();

        assert_eq!(err.kind, AdapterErrorKind::Measurement);
        assert!(!err.message.contains("secret-token"));
    }

    #[test]
    fn attachment_writer_rejects_invalid_media_type_as_measurement() {
        let dir = tempfile::tempdir().unwrap();
        let writer = FilesystemAttachmentWriter::new(dir.path(), policy());

        let err = writer
            .write_raw_payload(br#"{"ok":true}"#, "not a media type")
            .unwrap_err();

        assert_eq!(err.kind, AdapterErrorKind::Measurement);
    }

    #[test]
    fn attachment_writer_rejects_disallowed_media_type_as_measurement() {
        let dir = tempfile::tempdir().unwrap();
        let writer = FilesystemAttachmentWriter::new(dir.path(), policy());

        let err = writer
            .write_raw_payload(b"opaque-bytes", "text/plain")
            .unwrap_err();

        assert_eq!(err.kind, AdapterErrorKind::Measurement);
        assert_eq!(err.message, "unsupported attachment media type: text/plain");
    }

    #[test]
    fn attachment_writer_surfaces_storage_failure_as_infrastructure() {
        let dir = tempfile::tempdir().unwrap();
        let root_file = dir.path().join("not-a-directory");
        fs::write(&root_file, b"occupied").unwrap();
        let writer = FilesystemAttachmentWriter::new(root_file, policy());

        let err = writer
            .write_raw_payload(br#"{"ok":true}"#, "application/json")
            .unwrap_err();

        assert_eq!(err.kind, AdapterErrorKind::Infrastructure);
        assert!(!err.message.contains("{\"ok\":true}"));
    }

    #[test]
    fn attachment_writer_reuses_existing_digest_path() {
        let dir = tempfile::tempdir().unwrap();
        let writer = FilesystemAttachmentWriter::new(dir.path(), policy());
        let payload = br#"{"hello":"world"}"#;

        let first = writer
            .write_raw_payload(payload, "application/json")
            .unwrap();
        let second = writer
            .write_raw_payload(payload, "application/json")
            .unwrap();

        assert_eq!(first, second);
        assert_eq!(
            fs::read(writer.stored_path(&first.sha256)).unwrap(),
            payload
        );
    }
}
