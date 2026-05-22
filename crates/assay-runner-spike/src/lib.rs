//! Internal contracts for the Assay-Runner Phase 1 spike — legacy alias.
//!
//! This crate is deliberately publish-disabled. It is a thin compatibility
//! wrapper after the Phase 2D extraction work:
//!
//! - Phase 2D Slice 1 moved the data-structure half of the v0 schemas to
//!   [`assay_runner_schema`].
//! - Phase 2D Slice 2 moved orchestration, archive assembly, and layer
//!   normalizers to [`assay_runner_core`].
//! - Phase 2D Slice 6B redirected the last in-workspace consumer
//!   (`assay-cli`) to depend on `assay-runner-schema` and
//!   `assay-runner-core` directly. As of Slice 6B no production code
//!   depends on this crate any more; it is kept only as a legacy
//!   navigational alias for readers of pre-Slice-6B history.
//!
//! `scripts/ci/assay_runner_lane_check.py`'s `--self-test` enforces that
//! `assay-cli` does not re-introduce a dependency on this wrapper; any
//! such regression fails the lane-check helper. See
//! `docs/reference/runner/assay-consumes-runner-external.md` for the
//! Slice 6 design decisions.
//!
//! The `pub use` re-exports below are retained for the same legacy-alias
//! reason. Any future PR that wishes to delete this crate entirely should
//! first confirm zero `pub use assay_runner_spike::` or
//! `assay_runner_spike::Type` references anywhere in the workspace
//! (including off-tree consumers if any exist), update the boundary-map
//! ownership row, and remove the workspace member and dependency entries.

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
