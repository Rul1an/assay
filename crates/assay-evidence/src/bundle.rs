use crate::crypto::id::compute_run_root;
use crate::crypto::jcs;
use crate::types::EvidenceEvent;
use anyhow::{Context, Result};
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use tar::{Builder, Header};

#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub schema_version: u32,
    pub bundle_id: String,
    pub producer: crate::types::ProducerMeta,
    pub run_id: String,
    pub event_count: usize,
    pub run_root: String,
    pub algorithms: AlgorithmMeta,
    pub files: FileMap,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AlgorithmMeta {
    pub canon: String, // "jcs-rfc8785"
    pub hash: String,  // "sha256"
    pub root: String,  // "sha256(concat(event_id + \n))"
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FileMap {
    pub events: String,
}

pub struct BundleWriter<W: Write> {
    tar: Builder<GzEncoder<W>>,
    events: Vec<EvidenceEvent>,
}

impl<W: Write> BundleWriter<W> {
    /// Create a new deterministic bundle writer.
    /// Compression level: Best.
    /// Mtime: 0 (1970-01-01).
    pub fn new(writer: W) -> Self {
        // Deterministic Gzip: No name, No mtime, No OS
        // flate2 defines mtime 0 by default when using GzEncoder?
        // We verify this in tests.
        let encoder = GzEncoder::new(writer, Compression::best());
        let mut tar = Builder::new(encoder);
        // Force determinism in tar builder
        tar.mode(tar::HeaderMode::Deterministic);

        Self {
            tar,
            events: Vec::new(),
        }
    }

    pub fn add_event(&mut self, event: EvidenceEvent) {
        self.events.push(event);
    }

    /// Finalize the bundle:
    /// 1. Compute run_root from all events.
    /// 2. Generate Manifest.
    /// 3. Write manifest.json (first file).
    /// 4. Write events.ndjson (second file).
    /// 5. Finish tar.
    pub fn finish(mut self) -> Result<()> {
        let event_ids: Result<Vec<String>> = self
            .events
            .iter()
            .map(|e| crate::crypto::id::compute_event_id(e))
            .collect();
        let event_ids = event_ids?;

        let run_root = compute_run_root(&event_ids);

        // Assume first event carries producer/run metadata for manifest
        // In clean architecture, this should be passed explicitly.
        // For v1, we extract from event[0].
        let first = self.events.first().context("bundle is empty")?;

        let manifest = Manifest {
            schema_version: 1,
            bundle_id: run_root.clone(), // v1: bundle_id = run_root
            producer: first.producer.clone(),
            run_id: first.run.run_id.clone(),
            event_count: self.events.len(),
            run_root: run_root.clone(),
            algorithms: AlgorithmMeta {
                canon: "jcs-rfc8785".into(),
                hash: "sha256".into(),
                root: "sha256(concat(event_id + \"\\n\"))".into(),
            },
            files: FileMap {
                events: "events.ndjson".into(),
            },
        };

        // Prepare File Content (Canonical)
        let manifest_bytes = jcs::to_vec(&manifest)?;
        let mut events_bytes = Vec::new();
        for event in &self.events {
            events_bytes.extend_from_slice(&jcs::to_vec(event)?);
            events_bytes.push(b'\n');
        }

        // Add Manifest
        self.add_entry("manifest.json", &manifest_bytes)?;

        // Add Events
        self.add_entry("events.ndjson", &events_bytes)?;

        self.tar.finish()?;
        Ok(())
    }

    fn add_entry(&mut self, path: &str, data: &[u8]) -> Result<()> {
        let mut header = Header::new_gnu();
        header.set_path(path)?;
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_uid(0);
        header.set_gid(0);
        header.set_mtime(0); // Epoch
        header.set_username("assay")?;
        header.set_groupname("assay")?;
        header.set_cksum();

        self.tar.append(&header, data)?;
        Ok(())
    }
}

pub struct BundleReader<R: Read> {
    // Implementing reader logic in next step
    _phantom: std::marker::PhantomData<R>,
}
