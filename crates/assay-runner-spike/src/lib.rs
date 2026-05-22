//! Internal contracts for the Assay-Runner Phase 1 spike.
//!
//! This crate is deliberately publish-disabled. It is a thin compatibility
//! wrapper after the Phase 2D extraction work:
//!
//! - Phase 2D Slice 1 moved the data-structure half of the v0 schemas to
//!   [`assay_runner_schema`].
//! - Phase 2D Slice 2 moved orchestration, archive assembly, and layer
//!   normalizers to [`assay_runner_core`].
//!
//! Both moves are surfaced here through `pub use` so existing call sites
//! (notably `assay-cli`, fixture wrappers under `tests/fixtures/runner-spike/`,
//! and acceptance scripts under `scripts/ci/runner-spike-*.sh`) continue to
//! import via `assay_runner_spike::{Type}` unchanged.
//!
//! Future extraction slices may redirect those consumers to depend on the
//! schema and core crates directly; this wrapper exists only so the
//! relocation can happen incrementally without breaking the active
//! delegated proof path.

pub use assay_runner_schema::{
    ArchiveFile, ArchiveManifest, BindingWindow, CapabilitySurface, CapabilitySurfaceError,
    CgroupCorrelationStatus, CorrelationBinding, CorrelationReport, CorrelationReportError,
    CorrelationStatus, KernelLayerStatus, ObservationHealth, PolicyLayerStatus, SdkLayerEvent,
    SdkLayerStatus, ARCHIVE_MANIFEST_SCHEMA, CAPABILITY_SURFACE_PATH, CAPABILITY_SURFACE_SCHEMA,
    CORRELATION_REPORT_PATH, CORRELATION_REPORT_SCHEMA, EVENTS_PATH, KERNEL_LAYER_PATH,
    MANIFEST_PATH, OBSERVATION_HEALTH_PATH, OBSERVATION_HEALTH_SCHEMA, POLICY_LAYER_PATH,
    SDK_EVENT_SCHEMA, SDK_LAYER_PATH,
};

pub use assay_runner_core::{
    KernelLayerBuilder, KernelLayerCapture, KernelLayerError, PolicyLayerCapture, PolicyLayerError,
    PolicyLayerEvent, RunExecutionError, RunOutcome, RunSpec, RunSpecError, RunnerSpikeArchive,
    RunnerSpikeArchiveError, SdkLayerCapture, SdkLayerError, KERNEL_EVENT_SCHEMA,
    POLICY_EVENT_SCHEMA, RUN_EVENT_SCHEMA,
};
