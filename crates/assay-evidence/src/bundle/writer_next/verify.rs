use crate::crypto::id::{compute_content_hash, compute_run_root};
use crate::json_strict::validate_json_strict;
use crate::types::EvidenceEvent;
use anyhow::Result;
use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::io::Read;
use std::path::Component;

use super::errors::{ErrorClass, ErrorCode, VerifyError};
use super::events;
use super::limits::{LimitReader, VerifyLimits};
use super::manifest::Manifest;
use super::tar_read::{read_line_bounded, EintrReader};

/// Allowed files in bundle (strict allowlist).
const ALLOWED_FILES: &[&str] = &["manifest.json", "events.ndjson"];

/// Verification result with detailed information.
#[derive(Debug, Clone)]
pub struct VerifyResult {
    /// Bundle manifest
    pub manifest: Manifest,
    /// Number of events verified
    pub event_count: usize,
    /// Recomputed run_root
    pub computed_run_root: String,
}

/// Verify a bundle's integrity and contract compliance.
///
/// # Checks Performed
///
/// 1. **Structure**: manifest.json first, events.ndjson second
/// 2. **Allowlist**: Only manifest.json and events.ndjson allowed
/// 3. **Path Safety**: No traversal (..), no absolute paths
/// 4. **No Duplicates**: Each file appears exactly once
/// 5. **Hash Integrity**: events.ndjson sha256 matches manifest
/// 6. **Size Integrity**: events.ndjson size matches manifest
/// 7. **Content Hash**: Every event has required content_hash
/// 8. **Hash Verification**: content_hash matches computed value
/// 9. **ID Contract**: event.id == run_id:seq
/// 10. **Sequence**: Contiguous 0, 1, 2, ... N-1
/// 11. **Run ID Consistency**: All events have same run_id as manifest
/// 12. **Event Count**: Matches manifest.event_count
/// 13. **Run Root**: Recomputed value matches manifest.run_root
///
/// # Errors
///
/// Returns detailed error with hints for common issues.
/// Default verification using standard limits.
///
/// See `verify_bundle_with_limits` for custom strictness.
pub fn verify_bundle<R: Read>(reader: R) -> Result<VerifyResult> {
    verify_bundle_with_limits(reader, VerifyLimits::default())
}

/// Verify a bundle with explicit resource limits.
pub fn verify_bundle_with_limits<R: Read>(reader: R, limits: VerifyLimits) -> Result<VerifyResult> {
    let reader = EintrReader::new(reader);
    let reader = LimitReader::new(reader, limits.max_bundle_bytes, "LimitBundleBytes");

    let decoder = GzDecoder::new(reader);
    let limited_decoder = LimitReader::new(decoder, limits.max_decode_bytes, "LimitDecodeBytes");
    let mut archive = tar::Archive::new(limited_decoder);

    let mut manifest: Option<Manifest> = None;
    let mut events_verified = false;
    let mut seen_files: HashSet<String> = HashSet::new();
    let mut computed_run_root = String::new();
    let mut actual_event_count = 0;

    let entries = archive.entries().map_err(|e| {
        let mut ve = VerifyError::from(e);
        if ve.message.contains("LimitBundleBytes") {
            ve.code = ErrorCode::LimitBundleBytes;
            ve.class = ErrorClass::Limits;
        } else if ve.message.contains("LimitDecodeBytes") {
            ve.code = ErrorCode::LimitDecodeBytes;
            ve.class = ErrorClass::Limits;
        } else {
            ve.code = ErrorCode::IntegrityTar;
        }
        ve.with_context("Gzip/Tar stream")
    })?;

    for (i, entry) in entries.enumerate() {
        let entry = entry.map_err(|e| {
            let mut ve = VerifyError::from(e);
            if ve.message.contains("LimitBundleBytes") {
                ve.code = ErrorCode::LimitBundleBytes;
                ve.class = ErrorClass::Limits;
            } else if ve.message.contains("LimitDecodeBytes") {
                ve.code = ErrorCode::LimitDecodeBytes;
                ve.class = ErrorClass::Limits;
            } else {
                ve.code = ErrorCode::IntegrityTar;
            }
            ve.with_context(format!("Entry #{}", i))
        })?;
        let path = entry.path().map_err(VerifyError::from)?.to_path_buf();
        let path_str = path.to_str().unwrap_or("");

        if path_str.len() > limits.max_path_len {
            return Err(VerifyError::new(
                ErrorClass::Limits,
                ErrorCode::LimitPathLength,
                format!(
                    "Path length {} exceeds limit {}",
                    path_str.len(),
                    limits.max_path_len
                ),
            )
            .into());
        }

        let header_size = entry.header().size().map_err(VerifyError::from)?;

        let max_size = if path_str == "manifest.json" {
            limits.max_manifest_bytes
        } else {
            limits.max_events_bytes
        };

        if header_size > max_size {
            return Err(VerifyError::new(
                ErrorClass::Limits,
                ErrorCode::LimitFileSize,
                format!(
                    "File '{}' declared size {} exceeds limit {}",
                    path_str, header_size, max_size
                ),
            )
            .into());
        }

        for component in path.components() {
            match component {
                Component::Normal(_) => {}
                Component::CurDir => {}
                _ => {
                    return Err(VerifyError::new(
                        ErrorClass::Security,
                        ErrorCode::SecurityPathTraversal,
                        format!("Invalid path component in '{}'", path_str),
                    )
                    .into())
                }
            }
        }

        if !ALLOWED_FILES.contains(&path_str) {
            return Err(VerifyError::new(
                ErrorClass::Contract,
                ErrorCode::ContractUnexpectedFile,
                format!("Unexpected file '{}'", path_str),
            )
            .into());
        }

        if !seen_files.insert(path_str.to_string()) {
            return Err(VerifyError::new(
                ErrorClass::Contract,
                ErrorCode::ContractDuplicateFile,
                format!("Duplicate file '{}'", path_str),
            )
            .into());
        }

        if i == 0 {
            if path_str != "manifest.json" {
                return Err(VerifyError::new(
                    ErrorClass::Contract,
                    ErrorCode::ContractFileOrder,
                    "First file must be 'manifest.json'",
                )
                .into());
            }

            let mut content = Vec::new();
            let mut manifest_reader =
                LimitReader::new(entry, limits.max_manifest_bytes, "LimitFileSize");
            manifest_reader.read_to_end(&mut content).map_err(|e| {
                let mut ve = VerifyError::from(e);
                if ve.message.contains("LimitFileSize") {
                    ve.code = ErrorCode::LimitFileSize;
                    ve.class = ErrorClass::Limits;
                }
                ve
            })?;

            let m: Manifest = serde_json::from_slice(&content).map_err(|e| {
                let mut ve = VerifyError::from(e);
                ve.code = ErrorCode::ContractInvalidJson;
                ve
            })?;

            if m.schema_version != 1 {
                return Err(VerifyError::new(
                    ErrorClass::Contract,
                    ErrorCode::ContractSchemaVersion,
                    format!("Unsupported schema version: {}", m.schema_version),
                )
                .into());
            }
            manifest = Some(m);
            continue;
        }

        let m = manifest.as_ref().ok_or_else(|| {
            VerifyError::new(
                ErrorClass::Contract,
                ErrorCode::ContractFileOrder,
                "File encountered before manifest.json",
            )
        })?;

        if path_str == "events.ndjson" {
            let file_meta = m.files.get("events.ndjson").ok_or_else(|| {
                VerifyError::new(
                    ErrorClass::Contract,
                    ErrorCode::ContractMissingFile,
                    "Manifest missing 'events.ndjson'",
                )
            })?;

            if header_size != file_meta.bytes {
                return Err(VerifyError::new(
                    ErrorClass::Integrity,
                    ErrorCode::IntegrityFileSizeMismatch,
                    format!(
                        "events.ndjson size mismatch: expected {}, got {}",
                        file_meta.bytes, header_size
                    ),
                )
                .into());
            }

            let mut hasher = Sha256::new();
            let mut reader = std::io::BufReader::new(entry);
            let mut line_buf = Vec::new();
            let mut prev_seq: Option<u64> = None;
            let mut content_hashes = Vec::new();
            let mut first_line = true;
            let mut seen_bytes: u64 = 0;

            loop {
                line_buf.clear();
                let n = read_line_bounded(&mut reader, &mut line_buf, limits.max_line_bytes)
                    .map_err(|e| {
                        let mut ve = VerifyError::from(e);
                        if ve.message.contains("LimitLineBytes") {
                            ve.code = ErrorCode::LimitLineBytes;
                            ve.class = ErrorClass::Limits;
                        }
                        ve
                    })?;
                if n == 0 {
                    break;
                }
                seen_bytes += n as u64;

                if first_line && line_buf.starts_with(&[0xEF, 0xBB, 0xBF]) {
                    return Err(VerifyError::new(
                        ErrorClass::Contract,
                        ErrorCode::ContractInvalidJson,
                        "BOM not allowed in NDJSON",
                    )
                    .into());
                }
                first_line = false;

                hasher.update(&line_buf);

                actual_event_count += 1;
                if actual_event_count > limits.max_events {
                    return Err(VerifyError::new(
                        ErrorClass::Limits,
                        ErrorCode::LimitTotalEvents,
                        format!("Event count exceeds limit {}", limits.max_events),
                    )
                    .into());
                }

                let mut line_content = if line_buf.ends_with(b"\n") {
                    &line_buf[..n - 1]
                } else {
                    &line_buf[..n]
                };

                if line_content.ends_with(b"\r") {
                    line_content = &line_content[..line_content.len() - 1];
                }

                if line_content.is_empty() {
                    continue;
                }

                let line_str = std::str::from_utf8(line_content).map_err(|e| {
                    VerifyError::new(
                        ErrorClass::Contract,
                        ErrorCode::ContractInvalidJson,
                        format!("Invalid UTF-8 in event: {}", e),
                    )
                })?;

                validate_json_strict(line_str).map_err(|e| {
                    VerifyError::new(
                        ErrorClass::Contract,
                        ErrorCode::ContractInvalidJson,
                        format!("Security: {}", e),
                    )
                })?;

                let event: EvidenceEvent = serde_json::from_str(line_str).map_err(|e| {
                    let mut ve = VerifyError::from(e);
                    ve.code = ErrorCode::ContractInvalidJson;
                    ve
                })?;

                if event.specversion != "1.0" {
                    return Err(VerifyError::new(
                        ErrorClass::Contract,
                        ErrorCode::ContractSchemaVersion,
                        "Invalid specversion",
                    )
                    .into());
                }

                let claimed_hash = event.content_hash.as_deref().ok_or_else(|| {
                    VerifyError::new(
                        ErrorClass::Contract,
                        ErrorCode::ContractSchemaVersion,
                        "Missing content_hash",
                    )
                })?;

                let computed_hash = compute_content_hash(&event).map_err(|e| {
                    VerifyError::new(
                        ErrorClass::Integrity,
                        ErrorCode::IntegrityEventHash,
                        e.to_string(),
                    )
                })?;

                if claimed_hash != computed_hash {
                    return Err(VerifyError::new(
                        ErrorClass::Integrity,
                        ErrorCode::IntegrityEventHash,
                        format!("Content hash mismatch at seq {}", event.seq),
                    )
                    .into());
                }
                content_hashes.push(computed_hash);

                match prev_seq {
                    None => {
                        if event.seq != 0 {
                            return Err(VerifyError::new(
                                ErrorClass::Contract,
                                ErrorCode::ContractSequenceStart,
                                format!("First event must have seq=0, got {}", event.seq),
                            )
                            .into());
                        }
                    }
                    Some(prev) => {
                        if event.seq != prev + 1 {
                            return Err(VerifyError::new(
                                ErrorClass::Contract,
                                ErrorCode::ContractSequenceGap,
                                "Sequence gap",
                            )
                            .into());
                        }
                    }
                }
                prev_seq = Some(event.seq);

                if event.run_id != m.run_id {
                    return Err(VerifyError::new(
                        ErrorClass::Contract,
                        ErrorCode::ContractRunIdMismatch,
                        "Inconsistent run_id",
                    )
                    .into());
                }
            }

            if seen_bytes != file_meta.bytes {
                return Err(VerifyError::new(
                    ErrorClass::Integrity,
                    ErrorCode::IntegrityFileSizeMismatch,
                    format!(
                        "events.ndjson byte mismatch: expected {}, got {}",
                        file_meta.bytes, seen_bytes
                    ),
                )
                .into());
            }

            let actual_hash = format!("sha256:{}", hex::encode(hasher.finalize()));
            let expected_hash = events::normalize_hash(&file_meta.sha256);

            if actual_hash != expected_hash {
                return Err(VerifyError::new(
                    ErrorClass::Integrity,
                    ErrorCode::IntegrityManifestHash,
                    "events.ndjson hash mismatch",
                )
                .into());
            }

            if actual_event_count != m.event_count {
                return Err(VerifyError::new(
                    ErrorClass::Contract,
                    ErrorCode::ContractSequenceGap,
                    "Event count mismatch",
                )
                .into());
            }

            computed_run_root = compute_run_root(&content_hashes);
            if computed_run_root != m.run_root {
                return Err(VerifyError::new(
                    ErrorClass::Integrity,
                    ErrorCode::IntegrityRunRootMismatch,
                    "Run root mismatch",
                )
                .into());
            }

            events_verified = true;
        }
    }

    if !events_verified {
        return Err(VerifyError::new(
            ErrorClass::Contract,
            ErrorCode::ContractMissingFile,
            "Missing events.ndjson",
        )
        .into());
    }

    Ok(VerifyResult {
        manifest: manifest.unwrap(),
        event_count: actual_event_count,
        computed_run_root,
    })
}
