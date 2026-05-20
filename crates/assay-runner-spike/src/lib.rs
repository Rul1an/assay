//! Internal contracts for the Assay-Runner Phase 1 spike.
//!
//! This crate is deliberately publish-disabled. It freezes the v0 measured-run
//! contract shapes before orchestration, eBPF capture, or SDK shims are wired.

mod archive;
mod correlation;
mod health;
mod run;
mod surface;

pub use archive::{
    ArchiveFile, ArchiveManifest, RunnerSpikeArchive, RunnerSpikeArchiveError,
    ARCHIVE_MANIFEST_SCHEMA, CAPABILITY_SURFACE_PATH, CORRELATION_REPORT_PATH, EVENTS_PATH,
    KERNEL_LAYER_PATH, MANIFEST_PATH, OBSERVATION_HEALTH_PATH, POLICY_LAYER_PATH, SDK_LAYER_PATH,
};
pub use correlation::{
    BindingWindow, CorrelationBinding, CorrelationReport, CorrelationReportError,
    CorrelationStatus, CORRELATION_REPORT_SCHEMA,
};
pub use health::{
    CgroupCorrelationStatus, KernelLayerStatus, ObservationHealth, PolicyLayerStatus,
    SdkLayerStatus, OBSERVATION_HEALTH_SCHEMA,
};
pub use run::{RunExecutionError, RunOutcome, RunSpec, RunSpecError};
pub use surface::{CapabilitySurface, CapabilitySurfaceError, CAPABILITY_SURFACE_SCHEMA};
