pub mod bundle;
pub mod crypto;
pub mod diff;
pub mod json_strict;
pub mod lint;
pub mod mandate;
pub mod ndjson;
pub mod sanitize;
pub mod store;
pub mod trust_basis;
pub mod types;

// Convenience re-exports
pub use bundle::{
    verify_bundle, verify_bundle_with_limits, AlgorithmMeta, BundleInfo, BundleReader,
    BundleWriter, ErrorClass, ErrorCode, FileMeta, Manifest, VerifyError, VerifyLimits,
    VerifyLimitsOverrides, VerifyResult,
};
pub use lint::packs::{load_pack, load_packs, LoadedPack, PackError, PackSource};
pub use ndjson::{read_events, write_events, NdjsonEvents};
pub use store::config::{resolve_store_url, StoreConfig};
pub use store::{
    BundleMeta, BundleStore, ObjectStoreBundleStore, StoreError, StoreSpec, StoreStatus,
};
pub use trust_basis::{
    generate_trust_basis, to_canonical_json_bytes, TrustBasis, TrustBasisClaim, TrustBasisOptions,
    TrustClaimBoundary, TrustClaimId, TrustClaimLevel, TrustClaimSource,
};
pub use types::{Envelope, EvidenceEvent, ProducerMeta, SPEC_VERSION};

// Re-export bytes for CLI convenience
pub use bytes::Bytes;
