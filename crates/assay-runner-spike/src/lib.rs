//! Internal contracts for the Assay-Runner Phase 1 spike.
//!
//! This crate is deliberately publish-disabled. It freezes the v0 measured-run
//! contract shapes before orchestration, eBPF capture, or SDK shims are wired.
//!
//! Phase 2D Slice 1 moved the data-structure half of the v0 schemas to the
//! sibling crate [`assay_runner_schema`]. This crate re-exports those types
//! so existing call sites continue to compile unchanged. The runner archive
//! assembly, layer parsing, kernel/policy/SDK capture orchestration, and
//! run-spec semantics remain here until Phase 2D Slice 2 splits them into
//! `assay-runner-core`.

mod archive;
mod kernel;
mod policy;
mod run;
mod sdk;

// Re-exports of the v0 schema layer hosted by `assay-runner-schema`. The set
// of names re-exported here is the same set this crate exposed before Slice 1,
// so external consumers can continue to import every name unchanged.
pub use assay_runner_schema::{
    ArchiveFile, ArchiveManifest, BindingWindow, CapabilitySurface, CapabilitySurfaceError,
    CgroupCorrelationStatus, CorrelationBinding, CorrelationReport, CorrelationReportError,
    CorrelationStatus, KernelLayerStatus, ObservationHealth, PolicyLayerStatus, SdkLayerEvent,
    SdkLayerStatus, ARCHIVE_MANIFEST_SCHEMA, CAPABILITY_SURFACE_PATH, CAPABILITY_SURFACE_SCHEMA,
    CORRELATION_REPORT_PATH, CORRELATION_REPORT_SCHEMA, EVENTS_PATH, KERNEL_LAYER_PATH,
    MANIFEST_PATH, OBSERVATION_HEALTH_PATH, OBSERVATION_HEALTH_SCHEMA, POLICY_LAYER_PATH,
    SDK_EVENT_SCHEMA, SDK_LAYER_PATH,
};

// Assembly-side runner spike contracts that still live in this crate.
pub use archive::{RunnerSpikeArchive, RunnerSpikeArchiveError};
pub use kernel::{KernelLayerBuilder, KernelLayerCapture, KernelLayerError, KERNEL_EVENT_SCHEMA};
pub use policy::{PolicyLayerCapture, PolicyLayerError, PolicyLayerEvent, POLICY_EVENT_SCHEMA};
pub use run::{RunExecutionError, RunOutcome, RunSpec, RunSpecError, RUN_EVENT_SCHEMA};
pub use sdk::{SdkLayerCapture, SdkLayerError};
