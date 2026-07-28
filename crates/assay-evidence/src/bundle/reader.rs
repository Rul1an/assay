//! Bundle reader for evidence bundles.
//!
//! Provides a safe way to read and iterate over events from a bundle
//! without needing to handle tar/gzip internals.
//!
//! # Design Choice: Memory-Based (Option A)
//!
//! This implementation reads the entire events.ndjson into memory.
//! For v1, this is acceptable because:
//! - Bundles are typically <100MB
//! - Simplifies lifetime management
//! - Avoids streaming complexity
//!
//! For very large bundles (>1GB), consider tempfile-based streaming
//! or the `into_events()` consuming pattern in a future version.

use crate::bundle::writer::{
    check_entry_path_len, classify_reader_io, classify_strict_json, read_events_bounded,
    ErrorClass, ErrorCode, VerifyError,
};
use crate::bundle::writer::{verify_bundle_with_limits, Manifest, VerifyLimits};
use crate::json_strict::validate_json_strict_with_depth;
use crate::ndjson::NdjsonEvents;
use crate::types::EvidenceEvent;
use anyhow::{Context, Result};
use assay_common::limits::{LimitKind, LimitReader};
use flate2::read::GzDecoder;
use std::io::{BufReader, Cursor, Read};

/// Whether a bundle open performs integrity verification. Deliberately not an
/// `Option<VerifyLimits>`: the limits apply either way.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Verify {
    Yes,
    No,
}

/// Bundle reader for safe event iteration.
///
/// # Example
///
/// ```no_run
/// use assay_evidence::bundle::reader::BundleReader;
/// use std::fs::File;
///
/// let file = File::open("bundle.tar.gz").unwrap();
/// let reader = BundleReader::open(file).unwrap();
///
/// println!("Run ID: {}", reader.manifest().run_id);
/// println!("Events: {}", reader.manifest().event_count);
///
/// for event in reader.events() {
///     let event = event.unwrap();
///     println!("  [{}] {}", event.seq, event.type_);
/// }
/// ```
pub struct BundleReader {
    manifest: Manifest,
    events_content: Vec<u8>,
}

impl BundleReader {
    /// Open and verify a bundle, loading it into memory.
    ///
    /// # Cost, and when not to use this
    ///
    /// The whole decompressed `events.ndjson` is retained so [`Self::events`] can iterate it, and
    /// the only ceiling on that is `max_events_bytes` — 500 MiB, five times `max_bundle_bytes`.
    /// A small, highly compressible bundle therefore costs far more resident memory than its size
    /// suggests: 5.7 MB on disk, events decompressing to about half a gigabyte, measured at 530 MB
    /// peak against 33 MB for the same bundle through [`crate::bundle::verify_bundle`].
    ///
    /// **If you need the answer and not the events, call `verify_bundle` instead.** It runs the
    /// same checks and returns the manifest in its [`crate::bundle::VerifyResult`], without
    /// holding the stream. `assay evidence verify`, `evidence diff`'s baseline check and
    /// `evidence attest` each used this constructor and then took nothing or only the manifest;
    /// routing them to the streaming verifier is where the 16x came from.
    ///
    /// # Process
    ///
    /// 1. Verify bundle integrity (all checks from `verify_bundle`)
    /// 2. Extract manifest
    /// 3. Load events.ndjson into memory
    ///
    /// # Errors
    ///
    /// - Bundle verification fails
    /// - IO errors
    /// - Memory allocation fails (very large bundles)
    ///
    /// Open and verify a bundle, loading it into memory.
    pub fn open<R: Read>(reader: R) -> Result<Self> {
        Self::open_internal(reader, Verify::Yes, VerifyLimits::default())
    }

    /// Open and verify a bundle with custom verification limits.
    pub fn open_with_limits<R: Read>(reader: R, limits: VerifyLimits) -> Result<Self> {
        Self::open_internal(reader, Verify::Yes, limits)
    }

    /// Open a bundle without verification (for debugging/inspection).
    ///
    /// Skipping verification does not skip the limits. The whole set still applies to ingest,
    /// expansion and member reads; only the integrity check is omitted. Peeking at a manifest is
    /// not a reason to let an untrusted archive decide how much memory to take.
    pub fn open_unverified<R: Read>(reader: R) -> Result<Self> {
        Self::open_internal(reader, Verify::No, VerifyLimits::default())
    }

    /// Open a bundle without verification, under an explicit limit set.
    pub fn open_unverified_with_limits<R: Read>(reader: R, limits: VerifyLimits) -> Result<Self> {
        Self::open_internal(reader, Verify::No, limits)
    }

    /// Whether to verify is a separate question from what the limits are. Carrying both in one
    /// `Option<VerifyLimits>` is what let the unverified paths run unbounded: `None` read as
    /// "no limits" when it only ever meant "no verification".
    fn open_internal<R: Read>(reader: R, verify: Verify, limits: VerifyLimits) -> Result<Self> {
        let max_bundle_bytes = limits.max_bundle_bytes;
        // ADR-043 §1: the ceiling applies to the stream, before the input is materialized.
        // Reading first and checking afterwards means an oversized archive has already sized
        // the allocation, whatever the subsequent verification concludes.
        let mut buffer = Vec::new();
        let mut reader = LimitReader::new(reader, max_bundle_bytes, LimitKind::SourceBytes);
        reader
            .read_to_end(&mut buffer)
            .map_err(classify_reader_io)?;

        let manifest = match verify {
            Verify::Yes => {
                verify_bundle_with_limits(Cursor::new(&buffer), limits)
                    .context("Bundle verification failed")?
                    .manifest
            }
            Verify::No => {
                BundleInfo::peek_with_limits(Cursor::new(&buffer), limits)
                    .context("Failed to peek bundle manifest")?
                    .manifest
            }
        };

        // Second pass: extract events content.
        //
        // ADR-043 §1: a single byte ceiling is not the limit set. The compressed ceiling above
        // bounds what arrives; it says nothing about what that expands to. Without the decode
        // ceiling here a bomb well inside `max_bundle_bytes` expands unbounded, and on the peek
        // path this pass is the only consumption because verification never runs.
        let decoder = GzDecoder::new(Cursor::new(&buffer));
        let decoder = LimitReader::new(decoder, limits.max_decode_bytes, LimitKind::DecodedBytes);
        let mut archive = tar::Archive::new(decoder);

        let mut events_content = Vec::new();
        let mut events_found = false;

        for entry in archive.entries().map_err(classify_reader_io)? {
            let entry = entry.map_err(classify_reader_io)?;
            // Measure the path on the bytes the archive carries, before any conversion.
            // `to_string_lossy` emits three bytes for every invalid one, so a check on the
            // converted string measures something the archive never sent.
            check_entry_path_len(entry.path_bytes().len(), limits.max_path_len)?;
            let path = entry.path()?.to_string_lossy().to_string();

            if path == "events.ndjson" {
                events_found = true;
                // `LimitFileSize` rather than an invented tag: the classification vocabulary is
                // `ErrorCode`, and a per-member ceiling is what `verify.rs` already reports under
                // that code. A tag with no matching variant cannot be classified by any caller.
                let entry =
                    LimitReader::new(entry, limits.max_events_bytes, LimitKind::MemberBytes);
                read_events_bounded(entry, &mut events_content, limits)?;
                break;
            }
        }

        // An absent member is not an empty one. Accepting a bundle with no `events.ndjson` as a
        // bundle with zero events let a stripped archive read as a well-formed empty run.
        if !events_found {
            return Err(anyhow::Error::new(VerifyError::new(
                ErrorClass::Contract,
                ErrorCode::ContractMissingFile,
                "events.ndjson missing from bundle".to_string(),
            )));
        }

        Ok(Self {
            manifest,
            events_content,
        })
    }

    /// Get the bundle manifest.
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Get the run ID.
    pub fn run_id(&self) -> &str {
        &self.manifest.run_id
    }

    /// Get the event count.
    pub fn event_count(&self) -> usize {
        self.manifest.event_count
    }

    /// Get the run root hash.
    pub fn run_root(&self) -> &str {
        &self.manifest.run_root
    }

    /// Iterate over events.
    ///
    /// Returns an iterator that yields `Result<EvidenceEvent>` for each line.
    /// Events are already verified during `open()`, so errors here indicate
    /// a bug or memory corruption.
    pub fn events(&self) -> NdjsonEvents<BufReader<Cursor<&[u8]>>> {
        let cursor = Cursor::new(self.events_content.as_slice());
        let reader = BufReader::new(cursor);
        NdjsonEvents::new(reader)
    }

    /// Collect all events into a Vec.
    ///
    /// Convenience method when you need random access to events.
    pub fn events_vec(&self) -> Result<Vec<EvidenceEvent>> {
        self.events().collect()
    }

    /// Get raw events content (canonical NDJSON bytes).
    ///
    /// Useful for re-exporting or hashing.
    pub fn events_raw(&self) -> &[u8] {
        &self.events_content
    }
}

/// Info-only bundle inspection (manifest only, no event loading).
///
/// Faster than `BundleReader::open()` when you only need metadata.
pub struct BundleInfo {
    pub manifest: Manifest,
}

impl BundleInfo {
    /// Read only the manifest from a bundle.
    ///
    /// Does NOT verify event integrity.
    /// Use `BundleReader::open()` for full verification.
    pub fn peek<R: Read>(reader: R) -> Result<Self> {
        Self::peek_with_limits(reader, VerifyLimits::default())
    }

    /// Peek at a manifest under an explicit limit set.
    ///
    /// Peeking still expands the archive and still materializes a member, so it needs the decode
    /// and per-file ceilings even though it verifies nothing. Left unbounded, this was the one
    /// public entrypoint where a bomb met no guard at all.
    pub fn peek_with_limits<R: Read>(reader: R, limits: VerifyLimits) -> Result<Self> {
        // Snapshot the whole source before parsing, exactly as the verifier does. A `LimitReader`
        // placed under gzip and tar only ever sees the bytes a decoder asks for, and peek returns
        // the moment it has read `manifest.json` — so a valid bundle followed by an arbitrarily
        // large suffix never had its tail requested and passed a ceiling it plainly exceeds. The
        // ceiling has to bound the artifact, not the prefix a parser happened to consume.
        let mut source = Vec::new();
        LimitReader::new(reader, limits.max_bundle_bytes, LimitKind::SourceBytes)
            .read_to_end(&mut source)
            .map_err(classify_reader_io)
            .context("Bundle source")?;

        let decoder = LimitReader::new(
            GzDecoder::new(std::io::Cursor::new(&source)),
            limits.max_decode_bytes,
            LimitKind::DecodedBytes,
        );
        let mut archive = tar::Archive::new(decoder);

        for entry in archive.entries().map_err(classify_reader_io)? {
            let entry = entry.map_err(classify_reader_io)?;
            check_entry_path_len(entry.path_bytes().len(), limits.max_path_len)?;
            let path = entry.path()?.to_string_lossy().to_string();

            if path == "manifest.json" {
                // Read manifest to string for strict validation
                let mut entry =
                    LimitReader::new(entry, limits.max_manifest_bytes, LimitKind::MemberBytes);
                let mut content = String::new();
                entry
                    .read_to_string(&mut content)
                    .map_err(classify_reader_io)
                    .context("Failed to read manifest.json")?;

                // The caller's ceiling, not the module constant: peek validated the manifest but
                // ignored `max_json_depth`, so an unverified read applied a different budget than
                // a verified one on the same document.
                validate_json_strict_with_depth(&content, limits.max_json_depth)
                    .map_err(|e| classify_strict_json(e, "Manifest", limits.max_json_depth))?;

                let manifest: Manifest =
                    serde_json::from_str(&content).context("Failed to parse manifest.json")?;
                return Ok(Self { manifest });
            }
        }

        anyhow::bail!("Bundle missing manifest.json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundle::writer::BundleWriter;
    use crate::types::EvidenceEvent;
    use chrono::{TimeZone, Utc};

    #[test]
    fn test_reader_basic() {
        let bundle = create_test_bundle(3);
        let reader = BundleReader::open(Cursor::new(&bundle)).unwrap();

        assert_eq!(reader.event_count(), 3);
        assert_eq!(reader.run_id(), "run_test");

        let events: Vec<_> = reader.events().collect::<Result<Vec<_>>>().unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].seq, 0);
        assert_eq!(events[1].seq, 1);
        assert_eq!(events[2].seq, 2);
    }

    #[test]
    fn test_reader_events_vec() {
        let bundle = create_test_bundle(2);
        let reader = BundleReader::open(Cursor::new(&bundle)).unwrap();

        let events = reader.events_vec().unwrap();
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_bundle_info_peek() {
        let bundle = create_test_bundle(5);
        let info = BundleInfo::peek(Cursor::new(&bundle)).unwrap();

        assert_eq!(info.manifest.event_count, 5);
        assert_eq!(info.manifest.run_id, "run_test");
    }

    fn create_test_bundle(event_count: usize) -> Vec<u8> {
        let mut buffer = Vec::new();
        {
            let mut writer = BundleWriter::new(&mut buffer);
            for seq in 0..event_count {
                writer.add_event(create_event(seq as u64));
            }
            writer.finish().unwrap();
        }
        buffer
    }

    fn create_event(seq: u64) -> EvidenceEvent {
        let mut event = EvidenceEvent::new(
            "assay.test",
            "urn:assay:test",
            "run_test",
            seq,
            serde_json::json!({"seq": seq}),
        );
        event.time = Utc.timestamp_opt(1700000000, 0).unwrap();
        event
    }
}
