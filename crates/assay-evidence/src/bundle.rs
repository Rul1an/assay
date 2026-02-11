pub mod limits;
pub mod manifest;
pub mod reader;
pub mod verify;
pub mod write;
pub mod x_assay;

// Re-exports for convenience
pub use limits::{VerifyLimits, VerifyLimitsOverrides};
pub use manifest::{AlgorithmMeta, FileMeta, Manifest};
pub use reader::{BundleInfo, BundleReader};
pub use verify::{
    verify_bundle, verify_bundle_with_limits, ErrorClass, ErrorCode, VerifyError, VerifyResult,
};
pub use write::BundleWriter;
pub use x_assay::{BundleProvenance, ProvenanceInput, XAssayExtension};
