//! Runner orchestration, archive assembly, and layer normalizers for the
//! Assay-Runner candidate.
//!
//! This crate is the Phase 2D Slice 2 result of the Assay-Runner extraction
//! roadmap (see `docs/reference/runner/extraction-roadmap.md`). It hosts the
//! mechanics half of the Phase 1 measured-run path: `RunSpec` orchestration,
//! archive assembly and writing, and the kernel/policy/SDK layer normalizers.
//! The data-structure half lives in [`assay_runner_schema`] since Slice 1.
//!
//! The crate is `publish = false` until Slice 7 (repository extraction). It
//! does not depend on `assay-cli` (the cgroup placement extraction is a
//! separate Slice 3 boundary move), does not depend on `assay-evidence`,
//! and does not embed Assay-side artifact-verification semantics; runner
//! archives remain consumable through the existing Assay evidence path.

mod archive;
mod kernel;
mod path_projection;
mod policy;
mod redact;
mod redaction_key;
mod run;
mod sdk;

pub use archive::{RunnerSpikeArchive, RunnerSpikeArchiveError};
pub use kernel::{KernelLayerBuilder, KernelLayerCapture, KernelLayerError, KERNEL_EVENT_SCHEMA};
pub use path_projection::{
    project_filesystem_paths, DeclaredPathProjectionRules, DeclaredPathRule, PathProjection,
    PathProjectionMapping, UnmatchedPathSummary, PATH_PROJECTION_SCHEMA,
};
pub use policy::{PolicyLayerCapture, PolicyLayerError, PolicyLayerEvent, POLICY_EVENT_SCHEMA};
pub use redact::{rule_specs, RedactMode, RedactionTally, Redactor};
pub use redaction_key::{KeyScope, RedactionKey, ENV_KEY_FILE, KEY_FILE_PREFIX};
pub use run::{RunExecutionError, RunOutcome, RunSpec, RunSpecError, RUN_EVENT_SCHEMA};
pub use sdk::{SdkLayerCapture, SdkLayerError};
