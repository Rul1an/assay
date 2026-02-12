pub mod bundle;
pub mod crypto;
pub mod diff;
pub mod evaluation;
pub mod json_strict;
pub mod lint;
pub mod mandate;
pub mod ndjson;
pub mod sanitize;
pub mod store;
pub mod types;

// Convenience re-exports
pub use bundle::{
    verify_bundle, verify_bundle_with_limits, AlgorithmMeta, BundleInfo, BundleProvenance,
    BundleReader, BundleWriter, ErrorClass, ErrorCode, FileMeta, Manifest, ProvenanceInput,
    VerifyError, VerifyLimits, VerifyLimitsOverrides, VerifyResult, XAssayExtension,
};
pub use evaluation::{verify_evaluation, VerifyEvalResult};
pub use lint::packs::{load_pack, load_packs, LoadedPack, PackError, PackSource};
pub use ndjson::{read_events, write_events, NdjsonEvents};
pub use store::{BundleMeta, BundleStore, ObjectStoreBundleStore, StoreError, StoreSpec};
pub use types::{Envelope, EvidenceEvent, ProducerMeta, SPEC_VERSION};

// Re-export bytes for CLI convenience
pub use bytes::Bytes;
