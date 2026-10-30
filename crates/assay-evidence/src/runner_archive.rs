//! Read-only integrity verification of the existing Runner v0 archive.
//! Archive integrity is separate from layer admission, coverage and authenticity.

use std::collections::BTreeMap;
use std::io::{Cursor, Read};

use assay_common::limits::{LimitKind, LimitReader};
use assay_runner_schema::{
    ArchiveManifest, CapabilitySurface, CorrelationReport, ObservationHealth,
    ARCHIVE_MANIFEST_SCHEMA, CAPABILITY_SURFACE_PATH, CORRELATION_REPORT_PATH, EVENTS_PATH,
    KERNEL_LAYER_PATH, MANIFEST_PATH, OBSERVATION_HEALTH_PATH, POLICY_LAYER_PATH, SDK_LAYER_PATH,
};
use flate2::bufread::GzDecoder;
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};

use crate::bundle::writer::{
    apply_limit_class, check_entry_path_len, classify_limit, plain_member_violation,
    strict_json_error,
};
use crate::json_strict::validate_json_strict_with_depth;
use crate::{ErrorClass, ErrorCode, VerifyError, VerifyLimits};

const PAYLOADS: [&str; 7] = [
    CAPABILITY_SURFACE_PATH,
    CORRELATION_REPORT_PATH,
    EVENTS_PATH,
    KERNEL_LAYER_PATH,
    POLICY_LAYER_PATH,
    SDK_LAYER_PATH,
    OBSERVATION_HEALTH_PATH,
];

fn refusal(code: ErrorCode, message: &'static str) -> VerifyError {
    VerifyError::new(ErrorClass::Contract, code, message)
}

fn io_error(error: std::io::Error, fallback: ErrorCode) -> VerifyError {
    let limit = classify_limit(&error);
    // Do not print archive-controlled tar paths or decoder diagnostics.
    apply_limit_class(
        VerifyError::new(
            ErrorClass::Integrity,
            fallback,
            "Runner archive stream refused",
        ),
        limit,
        fallback,
    )
}

fn parse<T: DeserializeOwned>(bytes: &[u8], limits: VerifyLimits) -> Result<T, VerifyError> {
    let text = std::str::from_utf8(bytes).map_err(|_| {
        refusal(
            ErrorCode::ContractInvalidJson,
            "Runner metadata is not UTF-8",
        )
    })?;
    validate_json_strict_with_depth(text, limits.max_json_depth)
        .map_err(|err| strict_json_error(err, "Runner metadata", limits.max_json_depth))?;
    serde_json::from_str(text).map_err(|_| {
        refusal(
            ErrorCode::ContractInvalidJson,
            "Invalid Runner metadata shape",
        )
    })
}

/// Verify the original, complete compressed bytes against a raw lowercase SHA-256 and run id.
///
/// Metadata uses existing Runner schema validators. The four NDJSON members are verified as
/// bytes only: their event semantics, coverage, redaction and causal interpretation are not
/// admitted here. This does not make a Runner archive an evidence bundle or incident input.
/// No files are extracted or written. Limits apply before source, decoded and member reads.
pub fn verify_runner_archive<R: Read>(
    reader: R,
    expected_sha256: &str,
    expected_run_id: &str,
    limits: VerifyLimits,
) -> Result<(), VerifyError> {
    if expected_sha256.len() != 64
        || !expected_sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || expected_run_id.is_empty()
    {
        return Err(refusal(
            ErrorCode::ContractInvalidJson,
            "Invalid external Runner expectation",
        ));
    }
    let mut source = Vec::new();
    LimitReader::new(reader, limits.max_bundle_bytes, LimitKind::SourceBytes)
        .read_to_end(&mut source)
        .map_err(|e| io_error(e, ErrorCode::IntegrityIo))?;
    if hex::encode(Sha256::digest(&source)) != expected_sha256 {
        return Err(VerifyError::new(
            ErrorClass::Integrity,
            ErrorCode::IntegrityManifestHash,
            "Runner archive differs from the external digest",
        ));
    }

    let mut cursor = Cursor::new(source.as_slice());
    let decoder = GzDecoder::new(&mut cursor);
    let bounded = LimitReader::new(decoder, limits.max_decode_bytes, LimitKind::DecodedBytes);
    let mut archive = tar::Archive::new(bounded);
    let mut files = BTreeMap::<String, Vec<u8>>::new();
    for (index, entry) in archive
        .entries()
        .map_err(|e| io_error(e, ErrorCode::IntegrityTar))?
        .raw(true)
        .enumerate()
    {
        // Occurrences, not unique names: duplicates must not evade this ceiling.
        if index >= 8 {
            return Err(refusal(
                ErrorCode::ContractUnexpectedFile,
                "Too many Runner archive members",
            ));
        }
        let mut entry = entry.map_err(|e| io_error(e, ErrorCode::IntegrityTar))?;
        if let Some(reason) = plain_member_violation(entry.header()) {
            return Err(VerifyError::new(
                ErrorClass::Integrity,
                ErrorCode::IntegrityTar,
                reason,
            ));
        }
        let path = entry.path_bytes();
        check_entry_path_len(path.len(), limits.max_path_len)?;
        let name = std::str::from_utf8(&path).map_err(|_| {
            refusal(
                ErrorCode::ContractUnexpectedFile,
                "Invalid Runner member path",
            )
        })?;
        if name != MANIFEST_PATH && !PAYLOADS.contains(&name) {
            return Err(refusal(
                ErrorCode::ContractUnexpectedFile,
                "Unexpected Runner archive member",
            ));
        }
        if index == 0 && name != MANIFEST_PATH {
            return Err(refusal(
                ErrorCode::ContractFileOrder,
                "Runner manifest must be first",
            ));
        }
        if files.contains_key(name) {
            return Err(refusal(
                ErrorCode::ContractDuplicateFile,
                "Duplicate Runner archive member",
            ));
        }
        let name = name.to_owned();
        let ceiling = if name.ends_with(".json") {
            limits.max_manifest_bytes
        } else {
            limits.max_events_bytes
        };
        if entry.size() > ceiling {
            return Err(VerifyError::new(
                ErrorClass::Limits,
                ErrorCode::LimitFileSize,
                "Runner member exceeds its byte ceiling",
            ));
        }
        let mut bytes = Vec::new();
        LimitReader::new(&mut entry, ceiling, LimitKind::MemberBytes)
            .read_to_end(&mut bytes)
            .map_err(|e| io_error(e, ErrorCode::IntegrityTar))?;
        files.insert(name, bytes);
    }
    // Consume the gzip trailer and reject hidden content after tar's logical end.
    let mut tail = archive.into_inner();
    let mut buffer = [0; 8192];
    let mut padding_bytes = 0_u64;
    loop {
        let count = tail
            .read(&mut buffer)
            .map_err(|e| io_error(e, ErrorCode::IntegrityGzip))?;
        if count == 0 {
            break;
        }
        padding_bytes += count as u64;
        if buffer[..count].iter().any(|&b| b != 0) {
            return Err(VerifyError::new(
                ErrorClass::Integrity,
                ErrorCode::IntegrityTar,
                "Nonzero data after the Runner tar archive",
            ));
        }
    }
    drop(tail);
    // tar consumed the first zero header. Require the second end block and whole blocks
    // thereafter; tar's iterator alone also accepts EOF with no terminator at all.
    if padding_bytes < 512 || !padding_bytes.is_multiple_of(512) {
        return Err(VerifyError::new(
            ErrorClass::Integrity,
            ErrorCode::IntegrityTar,
            "Incomplete Runner tar terminator",
        ));
    }
    if cursor.position() != source.len() as u64 {
        return Err(VerifyError::new(
            ErrorClass::Integrity,
            ErrorCode::IntegrityGzip,
            "Data after the Runner gzip stream",
        ));
    }
    if files.len() != 8
        || !files.contains_key(MANIFEST_PATH)
        || PAYLOADS.iter().any(|name| !files.contains_key(*name))
    {
        return Err(refusal(
            ErrorCode::ContractMissingFile,
            "Missing Runner archive member",
        ));
    }
    let manifest: ArchiveManifest = parse(&files[MANIFEST_PATH], limits)?;
    if manifest.schema != ARCHIVE_MANIFEST_SCHEMA {
        return Err(refusal(
            ErrorCode::ContractSchemaVersion,
            "Unsupported Runner manifest schema",
        ));
    }
    if manifest.run_id != expected_run_id {
        return Err(refusal(
            ErrorCode::ContractRunIdMismatch,
            "Runner manifest run id mismatch",
        ));
    }
    if manifest.files.len() != 7
        || manifest
            .files
            .keys()
            .any(|name| !PAYLOADS.contains(&name.as_str()))
    {
        return Err(refusal(
            ErrorCode::ContractUnexpectedFile,
            "Runner manifest membership mismatch",
        ));
    }
    for name in PAYLOADS {
        let record = manifest.files.get(name).ok_or_else(|| {
            refusal(
                ErrorCode::ContractMissingFile,
                "Missing Runner manifest entry",
            )
        })?;
        let bytes = &files[name];
        if record.path != name {
            return Err(refusal(
                ErrorCode::ContractUnexpectedFile,
                "Runner manifest path mismatch",
            ));
        }
        if record.bytes != bytes.len() as u64 {
            return Err(VerifyError::new(
                ErrorClass::Integrity,
                ErrorCode::IntegrityFileSizeMismatch,
                "Runner payload length mismatch",
            ));
        }
        if record.sha256 != format!("sha256:{}", hex::encode(Sha256::digest(bytes))) {
            return Err(VerifyError::new(
                ErrorClass::Integrity,
                ErrorCode::IntegrityManifestHash,
                "Runner payload digest mismatch",
            ));
        }
    }
    let health: ObservationHealth = parse(&files[OBSERVATION_HEALTH_PATH], limits)?;
    let surface: CapabilitySurface = parse(&files[CAPABILITY_SURFACE_PATH], limits)?;
    let correlation: CorrelationReport = parse(&files[CORRELATION_REPORT_PATH], limits)?;
    if [&health.run_id, &surface.run_id, &correlation.run_id]
        .iter()
        .any(|id| id.as_str() != expected_run_id)
    {
        return Err(refusal(
            ErrorCode::ContractRunIdMismatch,
            "Runner metadata run id mismatch",
        ));
    }
    health.validate().map_err(|_| {
        refusal(
            ErrorCode::ContractInvalidEvent,
            "Invalid Runner observation health",
        )
    })?;
    surface.validate().map_err(|_| {
        refusal(
            ErrorCode::ContractInvalidEvent,
            "Invalid Runner capability surface",
        )
    })?;
    correlation.validate().map_err(|_| {
        refusal(
            ErrorCode::ContractInvalidEvent,
            "Invalid Runner correlation metadata",
        )
    })?;
    Ok(())
}

#[cfg(test)]
mod tests;
