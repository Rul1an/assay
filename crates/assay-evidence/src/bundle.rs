pub mod reader;
pub mod writer;

// Re-exports for convenience
pub use reader::{BundleInfo, BundleReader};
pub use writer::{verify_bundle, AlgorithmMeta, BundleWriter, FileMeta, Manifest, VerifyResult};
