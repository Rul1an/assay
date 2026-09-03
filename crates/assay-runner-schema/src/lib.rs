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
//! - `assay.runner.claim_support_parity.v0` (the fidelity gate's decisions per
//!   claim kind, published as a table so a rule in another language can be
//!   pinned against it rather than approximate it)
//!
//! The crate is published with the workspace release line. It has no eBPF,
//! monitor, CLI, fixture, filesystem-I/O, or projection-logic code; it is the
//! data half of the runner v0 contract layer.

mod archive_manifest;
mod claim_parity;
mod correlation;
mod coverage;
mod fidelity;
mod health;
mod sdk_event;
mod surface;

/// Shared claim vocabulary (ADR-048), re-exported under this crate's former names so
/// `assay_runner_schema::ClaimGateDecision` and `assay_runner_schema::CoverageClaimKind` keep
/// resolving. The tables that consume them stay in this crate.
pub use assay_common::claim::{ClaimDecision as ClaimGateDecision, ClaimKind as CoverageClaimKind};

pub use archive_manifest::{
    ArchiveFile, ArchiveManifest, ARCHIVE_MANIFEST_SCHEMA, CAPABILITY_SURFACE_PATH,
    CORRELATION_REPORT_PATH, EVENTS_PATH, KERNEL_LAYER_PATH, MANIFEST_PATH,
    OBSERVATION_HEALTH_PATH, POLICY_LAYER_PATH, SDK_LAYER_PATH,
};
pub use claim_parity::{
    all_claim_kinds, all_verdicts, claim_support, claim_support_table, permissiveness,
    ClaimSupport, ClaimSupportParityTable, ClaimSupportRow, ClaimSupportScope,
    CLAIM_SUPPORT_PARITY_SCHEMA,
};
pub use correlation::{
    BindingWindow, CorrelationBinding, CorrelationReport, CorrelationReportError,
    CorrelationStatus, CORRELATION_REPORT_SCHEMA,
};
pub use coverage::{
    CoverageClaimDecision, CoverageCompleteness, CoverageDescriptor, EffectDimension,
    COVERAGE_DESCRIPTOR_SCHEMA,
};
pub use fidelity::{
    RunnerClaimGate, RunnerFidelityReason, RunnerFidelityVerdict, RunnerFidelityVerdictReport,
    PROJECTION_CLAIM_LEVEL_INCONCLUSIVE, PROJECTION_CLAIM_LEVEL_PROJECTED_EQUIVALENT,
    PROJECTION_CLAIM_LEVEL_RAW_OBSERVED, RUNNER_FIDELITY_VERDICT_SCHEMA,
};
pub use health::{
    CaptureOrigin, CgroupCorrelationStatus, KernelLayerStatus, NetworkEndpointClaimScope,
    NetworkProtocolCoverageStatus, ObservationHealth, ObservationHealthError, PolicyLayerStatus,
    Redaction, RedactionReceipt, RedactionReceiptStatus, SdkLayerStatus, OBSERVATION_HEALTH_SCHEMA,
    REDACTION_RECEIPT_SCHEMA,
};
pub use sdk_event::{SdkLayerEvent, SDK_EVENT_SCHEMA};
pub use surface::{CapabilitySurface, CapabilitySurfaceError, CAPABILITY_SURFACE_SCHEMA};
