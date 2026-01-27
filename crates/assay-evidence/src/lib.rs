pub mod bundle;
pub mod crypto;
pub mod ndjson;
pub mod types;

// Convenience re-exports
pub use bundle::{
    verify_bundle, verify_bundle_with_limits, AlgorithmMeta, BundleInfo, BundleReader,
    BundleWriter, ErrorClass, ErrorCode, FileMeta, Manifest, VerifyError, VerifyLimits,
    VerifyResult,
};
pub use ndjson::{read_events, write_events, NdjsonEvents};
pub use types::{Envelope, EvidenceEvent, ProducerMeta, SPEC_VERSION};
