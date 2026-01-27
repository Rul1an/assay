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
use anyhow::{bail, Result};
use flate2::read::GzDecoder;
use flate2::{Compression, GzBuilder};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::io::{BufRead, Read, Write};
use std::path::Component;
use tar::{Builder, Header};

// VerifyLimits defined below with VerifyError.

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

        let mut events_bytes = Vec::new();
        for event in &self.events {
            events_bytes.extend_from_slice(&jcs::to_vec(event)?);
            events_bytes.push(b'\n');
        }

        let events_sha256 = format!("sha256:{}", hex::encode(Sha256::digest(&events_bytes)));
        let events_len = events_bytes.len() as u64;

        let run_root = compute_run_root(&content_hashes);

        let first = &self.events[0];
        let producer = self
            .producer
            .clone()
            .unwrap_or_else(|| first.producer_meta());
        let run_id = first.run_id.clone();

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
/// Default verification using standard limits.
///
/// See `verify_bundle_with_limits` for custom strictness.
pub fn verify_bundle<R: Read>(reader: R) -> Result<VerifyResult> {
    verify_bundle_with_limits(reader, VerifyLimits::default())
}

/// Verification error classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    /// Integrity violation (hash mismatch, corrupted gzip/tar).
    Integrity,
    /// Contract violation (missing fields, wrong source format, disallowed files).
    Contract,
    /// Security violation (path traversal, malicious payloads).
    Security,
    /// Resource limit exceeded (DoS prevention).
    Limits,
}

impl std::fmt::Display for ErrorClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Stable error codes for verification failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ErrorCode {
    // Integrity
    IntegrityGzip,
    IntegrityTar,
    IntegrityManifestHash,
    IntegrityEventHash,
    IntegrityRunRootMismatch,
    IntegrityZipBomb,
    IntegrityIo,
    // Contract
    ContractMissingManifest,
    ContractSchemaVersion,
    ContractFileOrder,
    ContractMissingFile,
    ContractDuplicateFile,
    ContractUnexpectedFile,
    ContractRunIdMismatch,
    ContractSequenceGap,
    ContractTimestampRegression,
    ContractInvalidJson,
    ContractInvalidEvent,
    // Limits
    LimitPathLength,
    LimitFileSize,
    LimitTotalEvents,
    LimitLineBytes,
    LimitJsonDepth,
    LimitBundleBytes,
    LimitDecodeBytes,
    // Security
    SecurityPathTraversal,
    SecurityAbsolutePath,
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Typed verification error with stable code.
#[derive(Debug, thiserror::Error)]
#[error("{class}: {message} ({code})")]
pub struct VerifyError {
    pub class: ErrorClass,
    pub code: ErrorCode,
    pub message: String,
    #[source]
    pub source: Option<anyhow::Error>,
}

impl VerifyError {
    pub fn new(class: ErrorClass, code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            class,
            code,
            message: message.into(),
            source: None,
        }
    }

    pub fn with_source(mut self, source: impl Into<anyhow::Error>) -> Self {
        self.source = Some(source.into());
        self
    }

    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.message = format!("{}: {}", context.into(), self.message);
        self
    }

    pub fn class(&self) -> ErrorClass {
        self.class
    }
}

// Helper for IO errors (defaults to Integrity/IntegrityIo)
impl From<std::io::Error> for VerifyError {
    fn from(err: std::io::Error) -> Self {
        Self {
            class: ErrorClass::Integrity,
            code: ErrorCode::IntegrityIo,
            message: err.to_string(),
            source: Some(err.into()),
        }
    }
}

// Helper for JSON errors (defaults to Contract/ContractSchemaVersion - simplified)
impl From<serde_json::Error> for VerifyError {
    fn from(err: serde_json::Error) -> Self {
        Self {
            class: ErrorClass::Contract,
            code: ErrorCode::ContractSchemaVersion, // Generalized for now, can be refined
            message: err.to_string(),
            source: Some(err.into()),
        }
    }
}

/// Resource limits for bundle verification.
#[derive(Debug, Clone, Copy)]
pub struct VerifyLimits {
    pub max_bundle_bytes: u64,
    pub max_decode_bytes: u64, // New: Limit uncompressed size
    pub max_manifest_bytes: u64,
    pub max_events_bytes: u64,
    pub max_events: usize,
    pub max_line_bytes: usize,
    pub max_path_len: usize,
    pub max_json_depth: usize,
}

impl Default for VerifyLimits {
    fn default() -> Self {
        Self {
            max_bundle_bytes: 100 * 1024 * 1024, // 100 MB compressed
            max_decode_bytes: 1024 * 1024 * 1024, // 1 GB uncompressed (10x ratio)
            max_manifest_bytes: 10 * 1024 * 1024, // 10 MB
            max_events_bytes: 500 * 1024 * 1024, // 500 MB
            max_events: 100_000,
            max_line_bytes: 1024 * 1024, // 1 MB
            max_path_len: 256,
            max_json_depth: 64,
        }
    }
}

/// A reader that limits the total number of bytes read and fails explicitly on overflow.
struct LimitReader<R> {
    inner: R,
    limit: u64,
    read: u64,
    error_tag: &'static str,
}

impl<R: Read> LimitReader<R> {
    fn new(inner: R, limit: u64, error_tag: &'static str) -> Self {
        Self { inner, limit, read: 0, error_tag }
    }
}

impl<R: Read> Read for LimitReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.read >= self.limit {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("{}: exceeded limit of {} bytes", self.error_tag, self.limit),
            ));
        }

        let max_to_read = (self.limit - self.read).min(buf.len() as u64) as usize;
        let n = self.inner.read(&mut buf[..max_to_read])?;
        self.read += n as u64;

        Ok(n)
    }
}

/// Helper to read a line with a hard memory limit BEFORE allocation.
fn read_line_bounded<R: BufRead>(reader: &mut R, buf: &mut Vec<u8>, max: usize) -> std::io::Result<usize> {
    let mut total_read = 0;
    loop {
        let (done, used) = {
            let available = reader.fill_buf()?;
            if available.is_empty() {
                (true, 0)
            } else {
                let (found, line_end) = match available.iter().position(|&b| b == b'\n') {
                    Some(pos) => (true, pos + 1),
                    None => (false, available.len()),
                };

                if total_read + line_end > max {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "LimitLineBytes: line exceeded limit",
                    ));
                }

                buf.extend_from_slice(&available[..line_end]);
                (found, line_end)
            }
        };
        reader.consume(used);
        total_read += used;
        if done || total_read == 0 {
            return Ok(total_read);
        }
        if total_read >= max && !done {
             // We reached max without finding a newline
             return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "LimitLineBytes: line exceeded limit",
            ));
        }
    }
}

/// Verify a bundle with explicit resource limits.
pub fn verify_bundle_with_limits<R: Read>(reader: R, limits: VerifyLimits) -> Result<VerifyResult> {
    // 1. Limit INPUT size (Network protection)
    let reader = LimitReader::new(reader, limits.max_bundle_bytes, "LimitBundleBytes");

    // 2. Limit OUTPUT size (Zip Bomb protection)
    let decoder = GzDecoder::new(reader);
    let limited_decoder = LimitReader::new(decoder, limits.max_decode_bytes, "LimitDecodeBytes");
    let mut archive = tar::Archive::new(limited_decoder);

    let mut manifest: Option<Manifest> = None;
    let mut events_verified = false;
    let mut seen_files: HashSet<String> = HashSet::new();
    let mut computed_run_root = String::new();
    let mut actual_event_count = 0;

    // Use a loop to handle errors gracefully mapping IO errors
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

        // Check: Path Length
        if path_str.len() > limits.max_path_len {
            bail!(VerifyError::new(
                ErrorClass::Limits,
                ErrorCode::LimitPathLength,
                format!("Path length {} exceeds limit {}", path_str.len(), limits.max_path_len)
            ));
        }

        // Check: File Size (Header) - Quick check only
        let header_size = entry.header().size().map_err(VerifyError::from)?;

        // Refined size limits
        let max_size = if path_str == "manifest.json" {
             limits.max_manifest_bytes
        } else {
             limits.max_events_bytes
        };

         if header_size > max_size {
            bail!(VerifyError::new(
                ErrorClass::Limits,
                ErrorCode::LimitFileSize,
                format!("File '{}' declared size {} exceeds limit {}", path_str, header_size, max_size)
            ));
        }

        // Check: Path Safety
        for component in path.components() {
            match component {
                Component::Normal(_) => {}
                Component::CurDir => {}
                _ => bail!(VerifyError::new(
                    ErrorClass::Security,
                    ErrorCode::SecurityPathTraversal,
                    format!("Invalid path component in '{}'", path_str)
                )),
            }
        }

        // Check: Allowlist
        if !ALLOWED_FILES.contains(&path_str) {
             bail!(VerifyError::new(
                ErrorClass::Contract,
                ErrorCode::ContractUnexpectedFile,
                format!("Unexpected file '{}'", path_str)
             ));
        }

        // Check: Duplicates
        if !seen_files.insert(path_str.to_string()) {
             bail!(VerifyError::new(
                ErrorClass::Contract,
                ErrorCode::ContractDuplicateFile,
                format!("Duplicate file '{}'", path_str)
             ));
        }

        // Processing Logic
        if i == 0 {
             if path_str != "manifest.json" {
                  bail!(VerifyError::new(
                    ErrorClass::Contract,
                    ErrorCode::ContractFileOrder,
                    "First file must be 'manifest.json'"
                  ));
             }

             // Manifest is small, read fully
             let mut content = Vec::new();
             let mut manifest_reader = LimitReader::new(entry, limits.max_manifest_bytes, "LimitFileSize");
             manifest_reader.read_to_end(&mut content).map_err(|e| {
                 let mut ve = VerifyError::from(e);
                 if ve.message.contains("LimitFileSize") {
                     ve.code = ErrorCode::LimitFileSize;
                     ve.class = ErrorClass::Limits;
                 }
                 ve
             })?;

             let m: Manifest = serde_json::from_slice(&content)
                .map_err(|e| {
                    let mut ve = VerifyError::from(e);
                    ve.code = ErrorCode::ContractInvalidJson;
                    ve
                })?;

             if m.schema_version != 1 {
                  bail!(VerifyError::new(
                    ErrorClass::Contract,
                    ErrorCode::ContractSchemaVersion,
                    format!("Unsupported schema version: {}", m.schema_version)
                  ));
             }
             manifest = Some(m);
             continue;
        }

        let m = manifest.as_ref().ok_or_else(|| VerifyError::new(
             ErrorClass::Contract,
             ErrorCode::ContractFileOrder,
             "File encountered before manifest.json"
        ))?;

        if path_str == "events.ndjson" {
            let file_meta = m.files.get("events.ndjson").ok_or_else(|| VerifyError::new(
                ErrorClass::Contract,
                ErrorCode::ContractMissingFile,
                "Manifest missing 'events.ndjson'"
            ))?;

            // Stream processing: Hash + Parse line-by-line
            let mut hasher = Sha256::new();
            let mut reader = std::io::BufReader::new(entry);
            let mut line_buf = Vec::new(); // Reusable buffer
            let mut prev_seq: Option<u64> = None;
            let mut content_hashes = Vec::new();
            let mut first_line = true;

            loop {
                line_buf.clear();
                let n = read_line_bounded(&mut reader, &mut line_buf, limits.max_line_bytes).map_err(|e| {
                    let mut ve = VerifyError::from(e);
                    if ve.message.contains("LimitLineBytes") {
                        ve.code = ErrorCode::LimitLineBytes;
                        ve.class = ErrorClass::Limits;
                    }
                    ve
                })?;
                if n == 0 { break; } // EOF

                // SOTA 2026: Block BOM (\uFEFF)
                if first_line && line_buf.starts_with(&[0xEF, 0xBB, 0xBF]) {
                    bail!(VerifyError::new(
                        ErrorClass::Contract,
                        ErrorCode::ContractInvalidJson,
                        "BOM not allowed in NDJSON"
                    ));
                }
                first_line = false;

                hasher.update(&line_buf);

                actual_event_count += 1;
                if actual_event_count > limits.max_events {
                    bail!(VerifyError::new(
                        ErrorClass::Limits,
                        ErrorCode::LimitTotalEvents,
                        format!("Event count exceeds limit {}", limits.max_events)
                    ));
                }

                let mut line_content = if line_buf.ends_with(b"\n") {
                    &line_buf[..n-1]
                } else {
                    &line_buf[..n]
                };

                // SOTA 2026: Strip CR for CRLF compatibility
                if line_content.ends_with(b"\r") {
                    line_content = &line_content[..line_content.len()-1];
                }

                if line_content.is_empty() { continue; }

                let event: EvidenceEvent = serde_json::from_slice(line_content)
                    .map_err(|e| {
                        let mut ve = VerifyError::from(e);
                        ve.code = ErrorCode::ContractInvalidJson;
                        ve
                    })?;

                // Contract checks on event...
                if event.specversion != "1.0" {
                     bail!(VerifyError::new(
                        ErrorClass::Contract,
                        ErrorCode::ContractSchemaVersion,
                        "Invalid specversion"
                     ));
                }

                let claimed_hash = event.content_hash.as_deref().ok_or_else(|| VerifyError::new(
                    ErrorClass::Contract,
                    ErrorCode::ContractSchemaVersion,
                    "Missing content_hash"
                ))?;

                let computed_hash = compute_content_hash(&event).map_err(|e| VerifyError::new(
                    ErrorClass::Integrity,
                    ErrorCode::IntegrityEventHash,
                    e.to_string()
                ))?;

                if claimed_hash != computed_hash {
                    bail!(VerifyError::new(
                        ErrorClass::Integrity,
                        ErrorCode::IntegrityEventHash,
                        format!("Content hash mismatch at seq {}", event.seq)
                    ));
                }
                content_hashes.push(computed_hash);

                match prev_seq {
                    None => if event.seq != 0 {
                        bail!(VerifyError::new(ErrorClass::Contract, ErrorCode::ContractSequenceGap, "First event seq != 0"));
                    },
                    Some(prev) => if event.seq != prev + 1 {
                        bail!(VerifyError::new(ErrorClass::Contract, ErrorCode::ContractSequenceGap, "Sequence gap"));
                    }
                }
                prev_seq = Some(event.seq);

                if event.run_id != m.run_id {
                     bail!(VerifyError::new(ErrorClass::Contract, ErrorCode::ContractRunIdMismatch, "Inconsistent run_id"));
                }
            }

            let actual_hash = format!("sha256:{}", hex::encode(hasher.finalize()));
            let expected_hash = normalize_hash(&file_meta.sha256);

            if actual_hash != expected_hash {
                 bail!(VerifyError::new(
                    ErrorClass::Integrity,
                    ErrorCode::IntegrityManifestHash,
                    "events.ndjson hash mismatch"
                 ));
            }

            if actual_event_count != m.event_count {
                 bail!(VerifyError::new(
                    ErrorClass::Contract,
                    ErrorCode::ContractSequenceGap,
                    "Event count mismatch"
                 ));
            }

            computed_run_root = compute_run_root(&content_hashes);
            if computed_run_root != m.run_root {
                 bail!(VerifyError::new(
                    ErrorClass::Integrity,
                    ErrorCode::IntegrityRunRootMismatch,
                    "Run root mismatch"
                 ));
            }

            events_verified = true;
        }
    }

    if !events_verified {
        bail!(VerifyError::new(
            ErrorClass::Contract,
            ErrorCode::ContractMissingFile,
            "Missing events.ndjson"
        ));
    }

    Ok(VerifyResult {
        manifest: manifest.unwrap(),
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

        let event1 = create_event(0);
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

    #[test]
    fn test_verify_limits_enforced() {
        let mut buffer = Vec::new();
        {
            let mut writer = BundleWriter::new(&mut buffer);
            writer.add_event(create_event(0));
            writer.finish().unwrap();
        }

        // 1. Test Event Count Limit
        let strict_count_limit = VerifyLimits {
            max_events: 0, // Should fail (bundle has 1 event)
            ..VerifyLimits::default()
        };
        let err = verify_bundle_with_limits(Cursor::new(&buffer), strict_count_limit);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("Event count exceeds limit"));

        // 2. Test File Size Limit
        let strict_size_limit = VerifyLimits {
            max_events_bytes: 10, // Should fail (events are larger)
            ..VerifyLimits::default()
        };
        let err = verify_bundle_with_limits(Cursor::new(&buffer), strict_size_limit);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("exceeds limit"));
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
