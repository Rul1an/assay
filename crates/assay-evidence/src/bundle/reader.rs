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

use crate::bundle::manifest::Manifest;
use crate::bundle::verify::verify_bundle_with_limits;
use crate::bundle::VerifyLimits;
use crate::json_strict::validate_json_strict;
use crate::ndjson::NdjsonEvents;
use crate::types::EvidenceEvent;
use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use std::io::{BufReader, Cursor, Read};

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
        Self::open_internal(reader, Some(VerifyLimits::default()))
    }

    /// Open and verify a bundle with custom verification limits.
    pub fn open_with_limits<R: Read>(reader: R, limits: VerifyLimits) -> Result<Self> {
        Self::open_internal(reader, Some(limits))
    }

    /// Open a bundle without verification (for debugging/inspection).
    pub fn open_unverified<R: Read>(reader: R) -> Result<Self> {
        Self::open_internal(reader, None)
    }

    fn open_internal<R: Read>(reader: R, limits: Option<VerifyLimits>) -> Result<Self> {
        // First pass: verify integrity and get manifest
        let mut buffer = Vec::new();
        let mut reader = reader;
        reader.read_to_end(&mut buffer)?;

        let manifest = if let Some(limits) = limits {
            let result = verify_bundle_with_limits(Cursor::new(&buffer), limits)
                .context("Bundle verification failed")?;
            result.manifest
        } else {
            // Peek only
            let info =
                BundleInfo::peek(Cursor::new(&buffer)).context("Failed to peek bundle manifest")?;
            info.manifest
        };

        // Second pass: extract events content
        let decoder = GzDecoder::new(Cursor::new(&buffer));
        let mut archive = tar::Archive::new(decoder);

        let mut events_content = Vec::new();

        for entry in archive.entries()? {
            let mut entry = entry?;
            let path = entry.path()?.to_string_lossy().to_string();

            if path == "events.ndjson" {
                entry.read_to_end(&mut events_content)?;
                break;
            }
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
        let decoder = GzDecoder::new(reader);
        let mut archive = tar::Archive::new(decoder);

        for entry in archive.entries()? {
            let mut entry = entry?;
            let path = entry.path()?.to_string_lossy().to_string();

            if path == "manifest.json" {
                // Read manifest to string for strict validation
                let mut content = String::new();
                entry
                    .read_to_string(&mut content)
                    .context("Failed to read manifest.json")?;

                // Security: Validate JSON strictly before parsing
                validate_json_strict(&content)
                    .context("Security: Invalid JSON in manifest.json")?;

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
    use crate::bundle::BundleWriter;
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
