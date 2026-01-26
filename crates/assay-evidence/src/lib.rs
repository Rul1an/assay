pub mod bundle;
pub mod crypto;
pub mod ndjson;
pub mod types;

// Convenience re-exports
pub use bundle::{
    verify_bundle, AlgorithmMeta, BundleInfo, BundleReader, BundleWriter, FileMeta, Manifest,
    VerifyResult,
};
pub use ndjson::{read_events, write_events, NdjsonEvents};
pub use types::{Envelope, EvidenceEvent, ProducerMeta, SPEC_VERSION};
