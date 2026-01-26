//! Evidence Bundle writer and verifier.
//!
//! A bundle is a deterministic tar.gz archive containing:
//! - `manifest.json`: Bundle metadata and integrity hashes
//! - `events.ndjson`: Canonical NDJSON event stream
//!
//! # Determinism Guarantees
//!
//! Bundles are byte-for-byte reproducible when:
//! - Same events (with deterministic timestamps)
//! - Same event order
//! - Same producer metadata
//!
//! # Verification Guarantees
//!
//! `verify_bundle` enforces:
//! - `content_hash` present on all events
//! - `run_root` matches recomputed value
//! - `event_count` matches actual count
//! - `run_id` consistent across all events
//! - Sequence is contiguous (0, 1, 2, ...)
//! - Only allowed files (manifest.json, events.ndjson)
//! - No path traversal or duplicates

use crate::crypto::id::{compute_content_hash, compute_run_root, compute_stream_id};
use crate::crypto::jcs;
use crate::types::{EvidenceEvent, ProducerMeta};
use anyhow::{bail, Context, Result};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::{Compression, GzBuilder};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::io::{BufRead, Cursor, Read, Write};
use std::path::Component;
use tar::{Builder, Header};

/// Bundle manifest (first file in archive).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Manifest {
    /// Schema version (always 1 for v1 contract)
    pub schema_version: u32,
    /// Bundle ID (equals run_root for v1)
    pub bundle_id: String,
    /// Producer metadata
    pub producer: ProducerMeta,
    /// Run identifier
    pub run_id: String,
    /// Total event count
    pub event_count: usize,
    /// Integrity chain root
    pub run_root: String,
    /// Algorithm specifications
    pub algorithms: AlgorithmMeta,
    /// File metadata (hash + size)
    pub files: BTreeMap<String, FileMeta>,
}

/// Algorithm metadata for verification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AlgorithmMeta {
    /// Canonicalization scheme
    pub canon: String,
    /// Hash algorithm
    pub hash: String,
    /// Run root computation
    pub root: String,
}

impl Default for AlgorithmMeta {
    fn default() -> Self {
        Self {
            canon: "jcs-rfc8785".into(),
            hash: "sha256".into(),
            root: "sha256(concat(content_hash + \"\\n\"))".into(),
        }
    }
}

/// File metadata within bundle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileMeta {
    /// Relative path within archive
    pub path: String,
    /// SHA-256 hash with prefix
    pub sha256: String,
    /// Size in bytes
    pub bytes: u64,
}

/// Allowed files in bundle (strict allowlist).
const ALLOWED_FILES: &[&str] = &["manifest.json", "events.ndjson"];

/// Deterministic bundle writer.
///
/// Collects events and writes a reproducible tar.gz bundle.
///
/// # Normalization
///
/// Before writing, the writer normalizes events:
/// - Computes `content_hash` if missing
/// - Validates `id` matches `run_id:seq`
///
/// # Example
///
/// ```no_run
/// use assay_evidence::bundle::BundleWriter;
/// use assay_evidence::types::EvidenceEvent;
/// use std::fs::File;
///
/// let file = File::create("bundle.tar.gz").unwrap();
/// let mut writer = BundleWriter::new(file);
///
/// // Add events...
/// // writer.add_event(event);
///
/// writer.finish().unwrap();
/// ```
pub struct BundleWriter<W: Write> {
    writer: Option<W>,
    events: Vec<EvidenceEvent>,
    producer: Option<ProducerMeta>,
}

impl<W: Write> BundleWriter<W> {
    /// Create a new deterministic bundle writer.
    pub fn new(writer: W) -> Self {
        Self {
            writer: Some(writer),
            events: Vec::new(),
            producer: None,
        }
    }

    /// Set producer metadata (optional, defaults to first event's producer).
    pub fn with_producer(mut self, producer: ProducerMeta) -> Self {
        self.producer = Some(producer);
        self
    }

    /// Add an event to the bundle.
    ///
    /// Events are normalized during `finish()`.
    pub fn add_event(&mut self, event: EvidenceEvent) {
        self.events.push(event);
    }

    /// Add multiple events.
    pub fn add_events(&mut self, events: impl IntoIterator<Item = EvidenceEvent>) {
        self.events.extend(events);
    }

    /// Finalize and write the bundle.
    ///
    /// # Process
    ///
    /// 1. Normalize events (compute content_hash)
    /// 2. Compute run_root
    /// 3. Generate manifest
    /// 4. Write manifest.json (first)
    /// 5. Write events.ndjson (second)
    /// 6. Finalize archive
    ///
    /// # Errors
    ///
    /// - Empty bundle (no events)
    /// - IO errors
    /// - Serialization errors
    pub fn finish(mut self) -> Result<()> {
        if self.events.is_empty() {
            bail!("Bundle is empty: at least one event required");
        }

        // Step 1: Normalize & Validate Logic (Sorted Order is CRITICAL)

        // SORT FIRST: Run Root must be computed on the canonical stream order
        self.events.sort_by_key(|e| e.seq);

        let mut content_hashes = Vec::with_capacity(self.events.len());

        // Grab expected values from first event (after sort)
        let first = &self.events[0];
        let run_id = first.run_id.clone();
        let first_source = first.source.clone();

        for (i, event) in self.events.iter_mut().enumerate() {
            // 1. Sequence Contiguity Check
            if event.seq != i as u64 {
                bail!(
                    "Sequence gap or mismatch at index {}.\n\
                     Found seq={} expected seq={}\n\
                     Hint: Bundle events must be contiguous from 0.",
                    i,
                    event.seq,
                    i
                );
            }

            // 2. Run ID Consistency
            if event.run_id != run_id {
                bail!(
                    "Inconsistent run_id at seq={}.\n\
                     Expected: {}\n\
                     Found: {}\n\
                     Hint: All events must have same run_id.",
                    event.seq,
                    run_id,
                    event.run_id
                );
            }

            // 3. Source Consistency (Bundle Scoped)
            if event.source != first_source {
                bail!(
                    "Inconsistent source at seq={}.\n\
                     Expected: {}\n\
                     Found: {}\n\
                     Hint: All events in a bundle must be from the same source.",
                    event.seq,
                    first_source,
                    event.source
                );
            }

            // 4. Source Format (URI)
            if !event.source.contains(':') || event.source.starts_with(':') {
                bail!(
                    "Invalid source format at seq={}.\n\
                     Value: '{}'\n\
                     Hint: source must be a URI (e.g. urn:assay:..., https://...).",
                    event.seq,
                    event.source
                );
            }

            // 5. Run ID Format (No colons)
            if event.run_id.contains(':') {
                bail!(
                    "Invalid run_id format at seq={}.\n\
                     Value: '{}'\n\
                     Hint: run_id cannot contain colons.",
                    event.seq,
                    event.run_id
                );
            }

            // 6. Compute/Verify Content Hash
            let hash = compute_content_hash(event)?;
            if let Some(existing) = &event.content_hash {
                if existing != &hash {
                    bail!("Event seq={} has inconsistent content_hash.", event.seq);
                }
            } else {
                event.content_hash = Some(hash.clone());
            }

            // 7. Verify ID Identity
            let expected_id = compute_stream_id(&event.run_id, event.seq);
            if event.id != expected_id {
                bail!("Event seq={} has incorrect id.", event.seq);
            }

            content_hashes.push(hash);
        }

        // Step 4: Serialize events to canonical NDJSON
        let mut events_bytes = Vec::new();
        for event in &self.events {
            events_bytes.extend_from_slice(&jcs::to_vec(event)?);
            events_bytes.push(b'\n');
        }

        let events_sha256 = format!("sha256:{}", hex::encode(Sha256::digest(&events_bytes)));
        let events_len = events_bytes.len() as u64;

        // Step 2: Compute run_root (Post-Sort)
        let run_root = compute_run_root(&content_hashes);

        // Step 3: Get metadata from first event (now robust)
        // (already have first from loop above, but re-grab to be safe if moved)
        let first = &self.events[0];
        let producer = self
            .producer
            .clone()
            .unwrap_or_else(|| first.producer_meta());
        let run_id = first.run_id.clone();

        // Step 5: Build manifest
        let mut files = BTreeMap::new();
        files.insert(
            "events.ndjson".into(),
            FileMeta {
                path: "events.ndjson".into(),
                sha256: events_sha256,
                bytes: events_len,
            },
        );

        let manifest = Manifest {
            schema_version: 1,
            bundle_id: run_root.clone(),
            producer,
            run_id,
            event_count: self.events.len(),
            run_root,
            algorithms: AlgorithmMeta::default(),
            files,
        };

        let manifest_bytes = jcs::to_vec(&manifest)?;

        // Step 6: Write deterministic tar.gz
        let writer = self.writer.take().unwrap();

        let encoder = GzBuilder::new()
            .mtime(0) // Epoch
            .operating_system(255) // Unknown (deterministic)
            .write(writer, Compression::best());

        let mut tar = Builder::new(encoder);
        tar.mode(tar::HeaderMode::Deterministic);

        // Manifest FIRST
        Self::write_entry(&mut tar, "manifest.json", &manifest_bytes)?;

        // Events SECOND
        Self::write_entry(&mut tar, "events.ndjson", &events_bytes)?;

        // Finalize
        let encoder = tar.into_inner()?;
        encoder.finish()?;

        Ok(())
    }

    fn write_entry<T: Write>(tar: &mut Builder<T>, path: &str, data: &[u8]) -> Result<()> {
        let mut header = Header::new_gnu();
        header.set_path(path)?;
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_uid(0);
        header.set_gid(0);
        header.set_mtime(0);
        header.set_username("assay")?;
        header.set_groupname("assay")?;
        header.set_cksum();

        tar.append(&header, data)?;
        Ok(())
    }
}

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
pub fn verify_bundle<R: Read>(reader: R) -> Result<VerifyResult> {
    let decoder = GzDecoder::new(reader);
    let mut archive = tar::Archive::new(decoder);

    let mut manifest: Option<Manifest> = None;
    let mut events_verified = false;
    let mut seen_files: HashSet<String> = HashSet::new();
    let mut computed_run_root = String::new();
    let mut actual_event_count = 0;

    let entries = archive.entries()?;
    for (i, entry) in entries.enumerate() {
        let mut entry = entry?;
        let path = entry.path()?.to_path_buf();
        let path_str = path.to_str().unwrap_or("");

        // Check 1: Path Safety (Component-based)
        for component in path.components() {
            match component {
                Component::Normal(_) => {} // OK
                Component::CurDir => {}    // OK (.)
                _ => {
                    bail!(
                        "Security: Invalid path component in '{}'\n\
                         Hint: Bundle contains path traversal or absolute path.",
                        path_str
                    );
                }
            }
        }

        // Check 2: Allowlist
        if !ALLOWED_FILES.contains(&path_str) {
            bail!(
                "Contract Violation: Unexpected file '{}'\n\
                 Allowed files: {:?}\n\
                 Hint: Bundle contains unauthorized files.",
                path_str,
                ALLOWED_FILES
            );
        }

        // Check 3: Duplicates
        if !seen_files.insert(path_str.to_string()) {
            bail!(
                "Contract Violation: Duplicate file '{}'\n\
                 Hint: Bundle contains same file multiple times.",
                path_str
            );
        }

        // Read content
        let mut content = Vec::new();
        entry.read_to_end(&mut content)?;

        // First entry MUST be manifest.json
        if i == 0 {
            if path_str != "manifest.json" {
                bail!(
                    "Contract Violation: First file must be 'manifest.json'\n\
                     Found: '{}'\n\
                     Hint: Bundle was not created with compliant writer.",
                    path_str
                );
            }

            let m: Manifest =
                serde_json::from_slice(&content).context("Failed to parse manifest.json")?;

            if m.schema_version != 1 {
                bail!(
                    "Unsupported schema version: {}\n\
                     Supported: 1\n\
                     Hint: This verifier only supports Evidence Contract v1.",
                    m.schema_version
                );
            }

            manifest = Some(m);
            continue;
        }

        // Manifest required before other files
        let m = manifest
            .as_ref()
            .context("Contract Violation: File before manifest")?;

        if path_str == "events.ndjson" {
            // Check 4: File Hash
            let file_meta = m
                .files
                .get("events.ndjson")
                .context("Manifest missing 'events.ndjson' entry")?;

            let actual_hash = format!("sha256:{}", hex::encode(Sha256::digest(&content)));
            let expected_hash = normalize_hash(&file_meta.sha256);

            if actual_hash != expected_hash {
                bail!(
                    "Integrity Error: events.ndjson hash mismatch\n\
                     Expected: {}\n\
                     Actual:   {}\n\
                     Hint: Bundle content was modified after creation.",
                    expected_hash,
                    actual_hash
                );
            }

            // Check 5: File Size
            if content.len() as u64 != file_meta.bytes {
                bail!(
                    "Integrity Error: events.ndjson size mismatch\n\
                     Expected: {} bytes\n\
                     Actual:   {} bytes",
                    file_meta.bytes,
                    content.len()
                );
            }

            // Verify events
            let cursor = Cursor::new(content);
            let lines = std::io::BufReader::new(cursor).lines();

            let mut content_hashes = Vec::new();
            let mut prev_seq: Option<u64> = None;

            for (line_idx, line) in lines.enumerate() {
                let line = line?;
                if line.trim().is_empty() {
                    continue;
                }

                let event: EvidenceEvent = serde_json::from_str(&line)
                    .with_context(|| format!("Invalid JSON at event line {}", line_idx))?;

                // Check 6: Spec Version
                if event.specversion != "1.0" {
                    bail!(
                        "Event {} has invalid specversion '{}'\n\
                         Expected: '1.0'",
                        line_idx,
                        event.specversion
                    );
                }

                // Check 7: Content Hash REQUIRED
                let claimed_hash = event.content_hash.as_deref().with_context(|| {
                    format!(
                        "Event {} (seq={}) missing required 'assaycontenthash'\n\
                         Hint: Re-export bundle with assay-evidence >= 2.6",
                        line_idx, event.seq
                    )
                })?;

                // Check 8: Content Hash Verification
                let computed_hash = compute_content_hash(&event)?;
                if claimed_hash != computed_hash {
                    bail!(
                        "Event {} (seq={}) content hash mismatch\n\
                         Claimed:  {}\n\
                         Computed: {}\n\
                         Hint: Event was modified after hash computation.",
                        line_idx,
                        event.seq,
                        claimed_hash,
                        computed_hash
                    );
                }

                content_hashes.push(computed_hash);

                // Check 9: ID Contract
                let expected_id = compute_stream_id(&event.run_id, event.seq);
                if event.id != expected_id {
                    bail!(
                        "Event {} ID mismatch\n\
                         Expected: '{}'\n\
                         Actual:   '{}'\n\
                         Hint: Event id must be 'run_id:seq'.",
                        line_idx,
                        expected_id,
                        event.id
                    );
                }

                // Check 10: Sequence Contiguity
                match prev_seq {
                    None => {
                        // First event must be seq=0
                        if event.seq != 0 {
                            bail!(
                                "Event {} has seq={}, expected seq=0\n\
                                 Hint: First event must have seq=0.",
                                line_idx,
                                event.seq
                            );
                        }
                    }
                    Some(prev) => {
                        if event.seq != prev + 1 {
                            bail!(
                                "Sequence gap at event {}: {} followed by {}\n\
                                 Expected: {}\n\
                                 Hint: Events must have contiguous sequence numbers.",
                                line_idx,
                                prev,
                                event.seq,
                                prev + 1
                            );
                        }
                    }
                }
                prev_seq = Some(event.seq);

                // Check 11: Run ID Consistency & Format
                if event.run_id != m.run_id {
                    bail!(
                        "Event {} has inconsistent run_id\n\
                         Manifest: '{}'\n\
                         Event:    '{}'\n\
                         Hint: All events must have same run_id as manifest.",
                        line_idx,
                        m.run_id,
                        event.run_id
                    );
                }

                if event.run_id.contains(':') {
                    bail!(
                        "Invalid run_id format at event {}\n\
                         Value: '{}'\n\
                         Hint: run_id must NOT contain colons.",
                        line_idx,
                        event.run_id
                    );
                }

                if !event.source.contains(':') {
                    bail!(
                        "Invalid source format at event {}\n\
                         Value: '{}'\n\
                         Hint: source must be a URI (contain ':').",
                        line_idx,
                        event.source
                    );
                }

                actual_event_count += 1;
            }

            // Check 12: Event Count
            if actual_event_count != m.event_count {
                bail!(
                    "Event count mismatch\n\
                     Manifest: {}\n\
                     Actual:   {}\n\
                     Hint: Manifest event_count doesn't match actual events.",
                    m.event_count,
                    actual_event_count
                );
            }

            // Check 13: Run Root
            computed_run_root = compute_run_root(&content_hashes);
            if computed_run_root != m.run_root {
                bail!(
                    "Run root mismatch\n\
                     Manifest: {}\n\
                     Computed: {}\n\
                     Hint: Event content or order was modified.",
                    m.run_root,
                    computed_run_root
                );
            }

            events_verified = true;
        }
    }

    // Final check: events.ndjson must exist
    if !events_verified {
        bail!(
            "Bundle missing 'events.ndjson'\n\
             Hint: Bundle is incomplete or corrupt."
        );
    }

    let manifest = manifest.unwrap();
    Ok(VerifyResult {
        manifest,
        event_count: actual_event_count,
        computed_run_root,
    })
}

/// Normalize hash to "sha256:..." format.
fn normalize_hash(hash: &str) -> String {
    if hash.starts_with("sha256:") {
        hash.to_string()
    } else {
        format!("sha256:{}", hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::EvidenceEvent;
    use chrono::{TimeZone, Utc};

    #[test]
    fn test_bundle_roundtrip() {
        let mut buffer = Vec::new();
        {
            let mut writer = BundleWriter::new(&mut buffer);
            writer.add_event(create_event(0));
            writer.add_event(create_event(1));
            writer.finish().unwrap();
        }

        let result = verify_bundle(Cursor::new(&buffer)).unwrap();
        assert_eq!(result.event_count, 2);
        assert_eq!(result.manifest.event_count, 2);
    }

    #[test]
    fn test_empty_bundle_fails() {
        let mut buffer = Vec::new();
        let writer = BundleWriter::new(&mut buffer);
        let err = writer.finish().unwrap_err();
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn test_inconsistent_run_id_fails() {
        let mut buffer = Vec::new();
        let mut writer = BundleWriter::new(&mut buffer);

        let mut event1 = create_event(0);
        let mut event2 = create_event(1);
        event2.run_id = "different_run".into();
        event2.id = "different_run:1".into();

        writer.add_event(event1);
        writer.add_event(event2);

        let err = writer.finish().unwrap_err();
        assert!(err.to_string().contains("run_id"));
    }

    #[test]
    fn test_manifest_first() {
        let mut buffer = Vec::new();
        {
            let mut writer = BundleWriter::new(&mut buffer);
            writer.add_event(create_event(0));
            writer.finish().unwrap();
        }

        // Manually check tar structure
        let decoder = GzDecoder::new(Cursor::new(&buffer));
        let mut archive = tar::Archive::new(decoder);
        let mut entries = archive.entries().unwrap();

        let first = entries.next().unwrap().unwrap();
        assert_eq!(first.path().unwrap().to_str().unwrap(), "manifest.json");

        let second = entries.next().unwrap().unwrap();
        assert_eq!(second.path().unwrap().to_str().unwrap(), "events.ndjson");
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
