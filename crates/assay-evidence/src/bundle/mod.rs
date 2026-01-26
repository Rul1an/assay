//! Evidence Bundle format and utilities.
//!
//! A bundle is a deterministic tar.gz archive containing:
//! - `manifest.json`: Metadata and integrity hashes
//! - `events.ndjson`: Canonical NDJSON event stream
//!
//! # Modules
//!
//! - [`writer`]: Create bundles with `BundleWriter`
//! - [`reader`]: Read bundles with `BundleReader`
//!
//! # Example
//!
//! ```no_run
//! use assay_evidence::bundle::{BundleWriter, BundleReader};
//! use assay_evidence::types::EvidenceEvent;
//! use std::io::Cursor;
//!
//! // Write
//! let mut buffer = Vec::new();
//! let mut writer = BundleWriter::new(&mut buffer);
//! // writer.add_event(event);
//! // writer.finish().unwrap();
//!
//! // Read
//! // let reader = BundleReader::open(Cursor::new(&buffer)).unwrap();
//! // for event in reader.events() { ... }
//! ```

pub mod reader;
pub mod writer;

// Re-exports for convenience
pub use reader::{BundleInfo, BundleReader};
pub use writer::{verify_bundle, AlgorithmMeta, BundleWriter, FileMeta, Manifest, VerifyResult};
