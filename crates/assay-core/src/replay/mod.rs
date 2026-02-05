//! Replay bundle (E9): hermetic artifact for "it works on my machine" reproducibility.
//!
//! This module provides the core types and writer for the Replay Bundle:
//! - **Manifest** (schema v1): digests, replay_coverage, toolchain, seeds, scrub policy, file manifest.
//! - **Bundle container**: .tar.gz with canonical layout (manifest.json, files/, outputs/, cassettes/).
//!
//! No user-facing CLI here; the CLI (`assay bundle create`, `assay replay --bundle`) is in E9c.
//! Scrubbing implementation is in E9b; this module only defines the scrub policy *field* (default deny).

pub mod bundle;
pub mod manifest;
pub mod toolchain;

pub use bundle::{build_file_manifest, bundle_digest, write_bundle_tar_gz, BundleEntry};
pub use manifest::{
    FileManifestEntry, ReplayCoverage, ReplayManifest, ReplayOutputs, ReplaySeeds, RunnerMeta,
    ScrubPolicy, ToolchainMeta, REPLAY_MANIFEST_SCHEMA_VERSION,
};
pub use toolchain::capture_toolchain;
