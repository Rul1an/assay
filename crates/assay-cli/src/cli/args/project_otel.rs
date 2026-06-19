use std::path::PathBuf;

/// Project assay runtime evidence into the OTel GenAI + OpenInference attribute view.
///
/// Read-only, one-directional, lossy: assay artifacts are the source of truth and the output is a
/// standards-shaped view of them. The CLI only reads files, deserializes JSON, and calls the library
/// projector; all projection semantics live in `assay_core::otel::projection`. See
/// `docs/reference/otel-projection.md`.
#[derive(clap::Args, Debug, Clone)]
pub struct ProjectOtelArgs {
    /// Path to an `assay.runner.capability_surface.v0` JSON file. Mutually exclusive with
    /// `--evidence-bundle`; exactly one input is required.
    #[arg(long = "capability-surface", conflicts_with = "evidence_bundle")]
    pub capability_surface: Option<PathBuf>,

    /// EXPERIMENTAL: project verified tool-decision-truth recipe rows from an evidence bundle (.tar.gz).
    /// The bundle is verified first; nothing is written unless every row verifies.
    #[arg(long = "evidence-bundle")]
    pub evidence_bundle: Option<PathBuf>,

    /// Optional path to an `assay.runner.observation_health.v0` JSON file.
    #[arg(long = "observation-health")]
    pub observation_health: Option<PathBuf>,

    /// Optional path to an `assay.enforcement_health.v0` JSON file. Absent means no enforcement
    /// claim is made (it does NOT assert that enforcement was absent — that lives in the carrier).
    #[arg(long = "enforcement-health")]
    pub enforcement_health: Option<PathBuf>,

    /// Write the projection JSON here instead of stdout. On success stdout stays empty.
    #[arg(long = "out")]
    pub out: Option<PathBuf>,
}
