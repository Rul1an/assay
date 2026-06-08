//! Versioned schema types and constants for the Assay-Runner v0 contracts.
//!
//! This crate is the Phase 2D Slice 1 result of the Assay-Runner extraction
//! roadmap (see `docs/reference/runner/extraction-roadmap.md`). It hosts the
//! data structures and constants for:
//!
//! - `assay.runner.observation_health.v0`
//! - `assay.runner.capability_surface.v0`
//! - `assay.runner.correlation_report.v0`
//! - `assay.runner.sdk_event.v0`
//! - `assay.runner.coverage_descriptor.v0` (internal helper contract; not an
//!   archive member yet)
//! - `assay.runner.archive_manifest.v0` (manifest semantics only; archive
//!   assembly mechanics live in `assay-runner-core` since Phase 2D Slice 2)
//!
//! The crate is `publish = false` until Slice 7 (repository extraction). It
//! has no eBPF, monitor, CLI, fixture, filesystem-I/O, or projection-logic
//! code; it is the data half of the runner v0 contract layer.

mod archive_manifest;
mod correlation;
mod coverage;
mod fidelity;
mod health;
mod sdk_event;
mod surface;

pub use archive_manifest::{
    ArchiveFile, ArchiveManifest, ARCHIVE_MANIFEST_SCHEMA, CAPABILITY_SURFACE_PATH,
    CORRELATION_REPORT_PATH, EVENTS_PATH, KERNEL_LAYER_PATH, MANIFEST_PATH,
    OBSERVATION_HEALTH_PATH, POLICY_LAYER_PATH, SDK_LAYER_PATH,
};
pub use correlation::{
    BindingWindow, CorrelationBinding, CorrelationReport, CorrelationReportError,
    CorrelationStatus, CORRELATION_REPORT_SCHEMA,
};
pub use coverage::{
    CoverageClaimDecision, CoverageClaimKind, CoverageCompleteness, CoverageDescriptor,
    EffectDimension, COVERAGE_DESCRIPTOR_SCHEMA,
};
pub use fidelity::{
    ClaimGateDecision, RunnerClaimGate, RunnerFidelityReason, RunnerFidelityVerdict,
    RunnerFidelityVerdictReport, PROJECTION_CLAIM_LEVEL_INCONCLUSIVE,
    PROJECTION_CLAIM_LEVEL_PROJECTED_EQUIVALENT, PROJECTION_CLAIM_LEVEL_RAW_OBSERVED,
    RUNNER_FIDELITY_VERDICT_SCHEMA,
};
pub use health::{
    CgroupCorrelationStatus, KernelLayerStatus, NetworkEndpointClaimScope,
    NetworkProtocolCoverageStatus, ObservationHealth, ObservationHealthError, PolicyLayerStatus,
    Redaction, SdkLayerStatus, OBSERVATION_HEALTH_SCHEMA,
};
pub use sdk_event::{SdkLayerEvent, SDK_EVENT_SCHEMA};
pub use surface::{CapabilitySurface, CapabilitySurfaceError, CAPABILITY_SURFACE_SCHEMA};
