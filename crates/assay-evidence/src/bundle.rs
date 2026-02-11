pub mod reader;
pub mod writer;

// Re-exports for convenience
pub use reader::{BundleInfo, BundleReader};
pub use writer::{
    verify_bundle, verify_bundle_with_limits, AlgorithmMeta, BundleWriter, ErrorClass, ErrorCode,
    FileMeta, Manifest, VerifyError, VerifyLimits, VerifyLimitsOverrides, VerifyResult,
};
