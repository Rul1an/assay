use crate::bundle::writer::{check_entry_path_len, strict_json_error};
use crate::crypto::id::{compute_content_hash, compute_run_root, compute_stream_id};
use crate::json_strict::validate_json_strict_with_depth;
use crate::types::EvidenceEvent;
use anyhow::Result;
use chrono::{DateTime, Utc};
use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::io::Read;
use std::path::Component;

use super::errors::{ErrorClass, ErrorCode, VerifyError};
use super::events;
use super::limits::{LimitReader, VerifyLimits};
use super::manifest::Manifest;
use super::stream_rules::{self, StreamRule};
use super::tar_read::{read_line_bounded, EintrReader};
use assay_common::limits::{LimitExceeded, LimitKind};

/// Recover the typed ceiling behind an `io::Error` and map it onto this crate's vocabulary.
///
/// Returns `None` when the failure was not a ceiling at all, so the caller keeps its own
/// classification. The lookup goes through the typed cause rather than the rendered message:
/// message text is a diagnostic, never a contract.
pub(crate) fn classify_limit(err: &std::io::Error) -> Option<ErrorCode> {
    let cause = LimitExceeded::from_io(err)?;
    // Exhaustive, with no wildcard. A new dimension in `LimitKind` must fail to compile here
    // rather than fall into a catch-all that reports the wrong code.
    Some(match cause.kind {
        LimitKind::SourceBytes => ErrorCode::LimitBundleBytes,
        LimitKind::DecodedBytes => ErrorCode::LimitDecodeBytes,
        LimitKind::MemberBytes => ErrorCode::LimitFileSize,
        LimitKind::LineBytes => ErrorCode::LimitLineBytes,
    })
}

/// Stamp a `VerifyError` with a ceiling classification, or with `fallback` when the failure was
/// not a ceiling. Taken as an already-resolved `Option` because the io error is consumed when the
/// `VerifyError` is built from it.
pub(crate) fn apply_limit_class(
    mut ve: VerifyError,
    limit: Option<ErrorCode>,
    fallback: ErrorCode,
) -> VerifyError {
    match limit {
        Some(code) => {
            ve.code = code;
            ve.class = ErrorClass::Limits;
        }
        None => ve.code = fallback,
    }
    ve
}

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

/// Everything the verification pass learned, including what the published result cannot carry.
///
/// Crate-internal on purpose. `VerifyResult` is published in 3.x with all fields public and no
/// `#[non_exhaustive]`, so adding a field to it stops every downstream struct literal and
/// exhaustive destructure from compiling. The window travels beside the result instead of inside
/// it, which costs one projection and no source break.
///
/// ADR-044's predicate needs the run's time window and the manifest does not carry it. It is
/// captured here because the verifier already parses every event, so the window is free and costs
/// no retention — the alternative was for the attestation path to walk the events again through
/// `BundleReader`, undoing the change that stopped verification holding the stream.
pub(crate) struct VerifiedBundle {
    pub(crate) result: VerifyResult,
    /// Earliest and latest event `time`, in that order.
    ///
    /// Not an `Option`: a zero-event bundle is refused, so a `VerifiedBundle` that exists has a
    /// first and a last event. Typed rather than rendered — the values are compared as instants
    /// and formatted once, at the edge that needs a string.
    pub(crate) time_window: (DateTime<Utc>, DateTime<Utc>),
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
/// 14. **Bundle ID**: manifest.bundle_id equals manifest.run_root
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
    Ok(verify_bundle_verbose_with_limits(reader, limits)?.result)
}

/// The same pass, returning what the published result cannot carry.
pub(crate) fn verify_bundle_verbose_with_limits<R: Read>(
    reader: R,
    limits: VerifyLimits,
) -> Result<VerifiedBundle> {
    // Snapshot the whole source under the ceiling before parsing anything.
    //
    // Streaming the ceiling into the gzip/tar walker only bounds the prefix those layers choose
    // to consume. They stop once the archive is logically complete, so a valid bundle followed by
    // an arbitrary suffix passed a ceiling far below the real input size: the trailing bytes were
    // never requested and therefore never counted. `read_to_end` through the same ceiling counts
    // everything the source actually holds.
    let mut source = Vec::new();
    LimitReader::new(
        EintrReader::new(reader),
        limits.max_bundle_bytes,
        LimitKind::SourceBytes,
    )
    .read_to_end(&mut source)
    .map_err(|e| {
        let limit = classify_limit(&e);
        apply_limit_class(VerifyError::from(e), limit, ErrorCode::IntegrityGzip)
            .with_context("Bundle source")
    })?;

    verify_bundle_snapshot(&source, limits)
}

/// Verify a bundle from bytes already bounded and materialized by the caller.
fn verify_bundle_snapshot(source: &[u8], limits: VerifyLimits) -> Result<VerifiedBundle> {
    let reader = std::io::Cursor::new(source);

    let decoder = GzDecoder::new(reader);
    let limited_decoder =
        LimitReader::new(decoder, limits.max_decode_bytes, LimitKind::DecodedBytes);
    let mut archive = tar::Archive::new(limited_decoder);

    let mut manifest: Option<Manifest> = None;
    let mut events_verified = false;
    let mut seen_files: HashSet<String> = HashSet::new();
    let mut computed_run_root = String::new();
    // Lines bound the work; events are what the manifest counts. Conflating them let a blank line
    // inflate `event_count` without contributing a content hash, so the chain held three entries
    // while the manifest claimed eight and both checks passed by measuring different things.
    let mut seen_lines: usize = 0;
    let mut actual_event_count = 0;
    let mut first_event: Option<EvidenceEvent> = None;
    // Min and max of `time` across events, for ADR-044's predicate. Compared as the instants they
    // already are: `event.time` is a `DateTime<Utc>`, so ordering it needs no rendering at all.
    // Rendering first and comparing the text was what discarded sub-second precision, because the
    // only rendering that made string order safe also truncated to whole seconds.
    let mut time_lo: Option<DateTime<Utc>> = None;
    let mut time_hi: Option<DateTime<Utc>> = None;

    let entries = archive.entries().map_err(|e| {
        let limit = classify_limit(&e);
        let ve = apply_limit_class(VerifyError::from(e), limit, ErrorCode::IntegrityTar);
        ve.with_context("Gzip/Tar stream")
    })?;

    for (i, entry) in entries.enumerate() {
        let entry = entry.map_err(|e| {
            let limit = classify_limit(&e);
            let ve = apply_limit_class(VerifyError::from(e), limit, ErrorCode::IntegrityTar);
            ve.with_context(format!("Entry #{}", i))
        })?;
        // Measure the path on the bytes the archive carries, before any conversion. `to_str`
        // returns None for invalid UTF-8 and the fallback measured an empty string, so a name
        // that is invalid on purpose skipped the ceiling entirely.
        check_entry_path_len(entry.path_bytes().len(), limits.max_path_len)?;

        let path = entry.path().map_err(VerifyError::from)?.to_path_buf();
        let path_str = path.to_str().unwrap_or("");

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
                // Value-free: the member name and its declared size are archive-controlled.
                // The dimension and the configured ceiling are what a reader can act on.
                format!(
                    "A declared member size exceeds the configured maximum of {max_size} bytes"
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
                LimitReader::new(entry, limits.max_manifest_bytes, LimitKind::MemberBytes);
            manifest_reader.read_to_end(&mut content).map_err(|e| {
                let limit = classify_limit(&e);
                let mut ve = VerifyError::from(e);
                if let Some(code) = limit {
                    ve.code = code;
                    ve.class = ErrorClass::Limits;
                }
                ve
            })?;

            // The manifest went straight to `serde_json` with no strict pass at all, so the
            // caller's nesting ceiling applied to events and not to the document that describes
            // them. Peek already validated it; the verifier did not.
            let manifest_str = std::str::from_utf8(&content).map_err(|e| {
                VerifyError::new(
                    ErrorClass::Contract,
                    ErrorCode::ContractInvalidJson,
                    format!("Invalid UTF-8 in manifest.json: {}", e),
                )
            })?;
            validate_json_strict_with_depth(manifest_str, limits.max_json_depth)
                .map_err(|e| strict_json_error(e, "Manifest", limits.max_json_depth))?;

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
                        let limit = classify_limit(&e);
                        let mut ve = VerifyError::from(e);
                        if let Some(code) = limit {
                            ve.code = code;
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

                seen_lines += 1;
                if seen_lines > limits.max_events {
                    return Err(VerifyError::new(
                        ErrorClass::Limits,
                        ErrorCode::LimitTotalEvents,
                        format!(
                            "Event count exceeds the configured maximum of {}",
                            limits.max_events
                        ),
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
                    // The writer emits exactly one event per line and never a blank one, so a
                    // blank line is a shape it cannot produce. Skipping it silently is what let
                    // it be counted as an event by the line counter above while contributing no
                    // content hash below.
                    return Err(VerifyError::new(
                        ErrorClass::Contract,
                        ErrorCode::ContractInvalidEvent,
                        "Blank line in events.ndjson",
                    )
                    .into());
                }

                actual_event_count += 1;

                let line_str = std::str::from_utf8(line_content).map_err(|e| {
                    VerifyError::new(
                        ErrorClass::Contract,
                        ErrorCode::ContractInvalidJson,
                        format!("Invalid UTF-8 in event: {}", e),
                    )
                })?;

                validate_json_strict_with_depth(line_str, limits.max_json_depth)
                    .map_err(|e| strict_json_error(e, "Event", limits.max_json_depth))?;

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

                // The stream rules the writer applies but the verifier did not: a consistent
                // `source` across events, and a `source` that is a URI. A bundle violating either
                // is one `BundleWriter::finish` refuses to emit, so accepting it here means the
                // two ends of the format disagree about what a bundle is. Checked against the
                // first event rather than the manifest because the manifest records no source.
                if let Some(first) = &first_event {
                    if let Some(rule) = stream_rules::violated_by_event(&event, first) {
                        return Err(VerifyError::new(
                            ErrorClass::Contract,
                            ErrorCode::ContractInvalidEvent,
                            format!("seq={}: {}", event.seq, rule.describe()),
                        )
                        .into());
                    }
                } else if !stream_rules::source_is_uri(&event.source) {
                    // First event: nothing to compare against yet, but the format rules still hold.
                    return Err(VerifyError::new(
                        ErrorClass::Contract,
                        ErrorCode::ContractInvalidEvent,
                        format!("seq={}: {}", event.seq, StreamRule::SourceIsUri.describe()),
                    )
                    .into());
                }
                if first_event.is_none() {
                    first_event = Some(event.clone());
                }

                let event_time = event.time;
                if time_lo.is_none_or(|lo| event_time < lo) {
                    time_lo = Some(event_time);
                }
                if time_hi.is_none_or(|hi| event_time > hi) {
                    time_hi = Some(event_time);
                }

                // Check 9, the ID contract. Documented at the top of this function since the
                // format was written, but never executed until ADR-043: the id is outside the
                // per-event content hash, so an id that disagrees with its own run_id and seq
                // survived every other check once the container was resealed. Ordered after the
                // run_id and seq checks so a mismatch here is always the id itself and not one of
                // its two inputs.
                //
                // What this does not close: the contract is internal, so a *consistent* rewrite
                // satisfies it. Rewrite the manifest run_id and every event's run_id and id
                // together and the bundle still verifies, because the chain root is computed over
                // content hashes that exclude identity by design. Binding a bundle to its identity
                // needs a second digest, not a stricter version of this check.
                // The id contract is `run_id:seq`, so it only means something if the split is
                // unambiguous. The writer refuses a run_id containing a colon for exactly this
                // reason; without the same rule here the verifier accepts bundles its own writer
                // cannot produce, and `a:b:0` reads equally well as run_id `a`, seq `b:0`.
                if event.run_id.contains(':') {
                    return Err(VerifyError::new(
                        ErrorClass::Contract,
                        ErrorCode::ContractInvalidEvent,
                        format!(
                            "run_id must not contain a colon; the id contract cannot be split unambiguously (seq={})",
                            event.seq
                        ),
                    )
                    .into());
                }

                if event.id != compute_stream_id(&event.run_id, event.seq) {
                    // Value-free: the diagnostic names the field and the position, never the
                    // offending id or the run_id it embeds. Both are attacker-controlled bundle
                    // content, and this message reaches CI logs. `seq` is an ordinal the
                    // neighbouring sequence errors already report, so it adds locality without
                    // echoing content.
                    return Err(VerifyError::new(
                        ErrorClass::Contract,
                        ErrorCode::ContractInvalidEvent,
                        format!(
                            "Event id does not match the run_id:seq contract at seq={}",
                            event.seq
                        ),
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

            // `BundleWriter::finish` bails on an empty event list, so a zero-event bundle is one
            // the writer cannot emit. Ordered before the count comparison so an empty bundle is
            // reported as empty rather than as a count that happens to agree with an empty
            // manifest.
            if actual_event_count == 0 {
                return Err(VerifyError::new(
                    ErrorClass::Contract,
                    ErrorCode::ContractInvalidEvent,
                    StreamRule::NonEmpty.describe(),
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

            // Check 14, the bundle id contract. The profile makes it normative -- "the manifest's
            // `run_root` and `bundle_id` MUST both equal it" -- but nothing derives one from the
            // other at read time, so the two could disagree and still verify. Ordered after the
            // chain check on purpose: a mutated run_root stays a root mismatch, and this fires
            // only when the chain is sound and its second copy in the manifest is not.
            //
            // Compared against the *computed* root rather than `m.run_root`. The two are equal
            // by the check above, so this is the same comparison today; it stops being the same
            // comparison the moment someone reorders these checks, and a contract that is only
            // sound because of what runs before it is one edit away from being decorative.
            if m.bundle_id != computed_run_root {
                return Err(VerifyError::new(
                    ErrorClass::Contract,
                    ErrorCode::ContractBundleIdMismatch,
                    "bundle_id must equal run_root",
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

    // Defense in depth: drain the remaining gzip stream so the decoder validates its CRC32/ISIZE
    // trailer. The tar reader stops after the expected entries and would otherwise never read the
    // trailer, so a mutation in the compressed stream that lands in a manifest field not covered by
    // a specific check (e.g. producer metadata) could slip through. Draining forces the CRC check.
    let mut tail = archive.into_inner();
    let mut drain = [0u8; 8192];
    loop {
        match tail.read(&mut drain) {
            Ok(0) => break,
            Ok(_) => continue,
            Err(e) => {
                let limit = classify_limit(&e);
                let mut ve = VerifyError::from(e);
                if let Some(code) = limit {
                    ve.code = code;
                    ve.class = ErrorClass::Limits;
                } else {
                    ve.code = ErrorCode::IntegrityGzip;
                    ve.class = ErrorClass::Integrity;
                }
                return Err(ve.with_context("Gzip trailer").into());
            }
        }
    }

    // The zero-event refusal above is what makes this pair total. Stated as an error rather than
    // an `unwrap`, so that if some future path ever reaches here without events it fails in the
    // verifier's own vocabulary instead of aborting the process.
    let time_window = time_lo.zip(time_hi).ok_or_else(|| {
        VerifyError::new(
            ErrorClass::Contract,
            ErrorCode::ContractInvalidEvent,
            StreamRule::NonEmpty.describe(),
        )
    })?;

    Ok(VerifiedBundle {
        result: VerifyResult {
            manifest: manifest.unwrap(),
            event_count: actual_event_count,
            computed_run_root,
        },
        time_window,
    })
}
