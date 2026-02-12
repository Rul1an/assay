//! Deterministic bundle writer.
//!
//! Byte-for-byte reproducible when same events + deterministic timestamps.

use crate::bundle::manifest::{AlgorithmMeta, FileMeta, Manifest};
use crate::bundle::x_assay::{ProvenanceInput, XAssayExtension};
use crate::crypto::id::{compute_content_hash, compute_run_root, compute_stream_id};
use crate::crypto::jcs;
use crate::types::{EvidenceEvent, ProducerMeta};
use anyhow::{bail, Context, Result};
use flate2::{Compression, GzBuilder};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Write;
use tar::{Builder, Header};

/// Deterministic bundle writer.
///
/// Collects events and writes a reproducible tar.gz bundle.
///
/// # Example
///
/// ```no_run
/// use assay_evidence::bundle::BundleWriter;
/// use assay_evidence::types::EvidenceEvent;
/// use std::fs::File;
///
/// let event = EvidenceEvent::new(
///     "assay.example.event",
///     "urn:assay:example",
///     "run_example",
///     0,
///     serde_json::json!({"example": true}),
/// );
///
/// let file = File::create("bundle.tar.gz").unwrap();
/// let mut writer = BundleWriter::new(file);
/// writer.add_event(event);
/// writer.finish().unwrap();
/// ```
pub struct BundleWriter<W: Write> {
    writer: Option<W>,
    events: Vec<EvidenceEvent>,
    producer: Option<ProducerMeta>,
    x_assay: Option<XAssayExtension>,
    provenance_input: Option<ProvenanceInput>,
}

impl<W: Write> BundleWriter<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer: Some(writer),
            events: Vec::new(),
            producer: None,
            x_assay: None,
            provenance_input: None,
        }
    }

    /// Set provenance input (ADR-025 E2 Phase 2). Mutually exclusive with `with_x_assay`.
    pub fn with_provenance(mut self, input: ProvenanceInput) -> Self {
        self.x_assay = None;
        self.provenance_input = Some(input);
        self
    }

    /// Set x-assay extension (ADR-025 E2). Mutually exclusive with `with_provenance`.
    pub fn with_x_assay(mut self, x_assay: XAssayExtension) -> Self {
        self.provenance_input = None;
        self.x_assay = Some(x_assay);
        self
    }

    pub fn with_producer(mut self, producer: ProducerMeta) -> Self {
        self.producer = Some(producer);
        self
    }

    pub fn add_event(&mut self, event: EvidenceEvent) {
        self.events.push(event);
    }

    pub fn add_events(&mut self, events: impl IntoIterator<Item = EvidenceEvent>) {
        self.events.extend(events);
    }

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

        let x_assay = if let Some(ref prov) = self.provenance_input {
            let created_at: String = prov
                .created_at
                .clone()
                .unwrap_or_else(|| first.time.to_rfc3339());
            let logical_digest_input = serde_json::json!({
                "run_root": run_root,
                "algorithms": AlgorithmMeta::default(),
                "files": &files,
            });
            let digest = format!(
                "sha256:{}",
                hex::encode(Sha256::digest(&jcs::to_vec(&logical_digest_input)?))
            );
            Some(prov.build_x_assay(&digest, &created_at))
        } else {
            if let Some(ref x) = self.x_assay {
                x.validate_safety().context("invalid x-assay extension")?;
            }
            self.x_assay.clone()
        };

        let manifest = Manifest {
            schema_version: 1,
            bundle_id: run_root.clone(),
            producer,
            run_id,
            event_count: self.events.len(),
            run_root,
            algorithms: AlgorithmMeta::default(),
            files,
            x_assay,
        };

        let manifest_bytes = jcs::to_vec(&manifest)?;

        let writer = self.writer.take().unwrap();
        Self::write_bundle_to(writer, &manifest_bytes, &events_bytes)?;

        Ok(())
    }

    fn write_bundle_to<Out: Write>(
        writer: Out,
        manifest_bytes: &[u8],
        events_bytes: &[u8],
    ) -> Result<()> {
        let encoder = GzBuilder::new()
            .mtime(0)
            .operating_system(255)
            .write(writer, Compression::best());
        let mut tar = Builder::new(encoder);
        tar.mode(tar::HeaderMode::Deterministic);
        Self::write_entry(&mut tar, "manifest.json", manifest_bytes)
            .context("writing manifest to tar")?;
        Self::write_entry(&mut tar, "events.ndjson", events_bytes)
            .context("writing events to tar")?;
        let encoder = tar.into_inner().context("finalizing tar archive")?;
        encoder.finish().context("compressing gzip stream")?;
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
