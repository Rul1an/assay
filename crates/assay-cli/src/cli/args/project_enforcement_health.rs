use std::path::PathBuf;

/// Project one existing `assay.enforcement_health.v0` or `.v1` document.
///
/// Read-only, one-directional, lossy. Absence of a projection is no claim and
/// never a pass. `--input` is required: no input means no document.
#[derive(clap::Args, Debug, Clone)]
pub struct ProjectEnforcementHealthArgs {
    /// Output format. Only `json` is accepted.
    #[arg(long = "format", value_parser = ["json"])]
    pub format: String,

    /// Path to exactly one existing `assay.enforcement_health.v0` or `.v1` JSON document.
    #[arg(long = "input")]
    pub input: PathBuf,
}
