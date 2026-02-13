use crate::crypto::id::{compute_content_hash, compute_run_root, compute_stream_id};
use crate::types::{EvidenceEvent, ProducerMeta};
use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;
use std::io::Write;

use super::manifest::{AlgorithmMeta, FileMeta, Manifest};
use super::{events, tar_write};

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

        let first = &self.events[0];
        let run_id = first.run_id.clone();
        let first_source = first.source.clone();

        for (i, event) in self.events.iter_mut().enumerate() {
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

            if !event.source.contains(':') || event.source.starts_with(':') {
                bail!(
                    "Invalid source format at seq={}.\n\
                     Value: '{}'\n\
                     Hint: source must be a URI (e.g. urn:assay:..., https://...).",
                    event.seq,
                    event.source
                );
            }

            if event.run_id.contains(':') {
                bail!(
                    "Invalid run_id format at seq={}.\n\
                     Value: '{}'\n\
                     Hint: run_id cannot contain colons.",
                    event.seq,
                    event.run_id
                );
            }

            let hash = compute_content_hash(event)?;
            if let Some(existing) = &event.content_hash {
                if existing != &hash {
                    bail!("Event seq={} has inconsistent content_hash.", event.seq);
                }
            } else {
                event.content_hash = Some(hash.clone());
            }

            let expected_id = compute_stream_id(&event.run_id, event.seq);
            if event.id != expected_id {
                bail!("Event seq={} has incorrect id.", event.seq);
            }

            content_hashes.push(hash);
        }

        let events_bytes = events::serialize_events_ndjson(&self.events)?;
        let events_sha256 = events::sha256_prefixed(&events_bytes);
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

        let manifest_bytes = crate::crypto::jcs::to_vec(&manifest)?;

        let writer = self.writer.take().unwrap();
        let mut tar = tar_write::create_deterministic_tar(writer);

        tar_write::write_entry(&mut tar, "manifest.json", &manifest_bytes)
            .context("writing manifest to tar")?;
        tar_write::write_entry(&mut tar, "events.ndjson", &events_bytes)
            .context("writing events to tar")?;

        let encoder = tar.into_inner().context("finalizing tar archive")?;
        encoder.finish().context("compressing gzip stream")?;

        Ok(())
    }
}
