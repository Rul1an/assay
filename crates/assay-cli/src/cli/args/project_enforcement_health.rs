use std::path::PathBuf;

/// Project one existing enforcement-health document or verified degradation bundle.
///
/// Read-only, one-directional, lossy. Absence of a projection is no claim and
/// never a pass. `--input` is required: no input means no document.
#[derive(clap::Args, Debug, Clone)]
pub struct ProjectEnforcementHealthArgs {
    /// Output format. Only `json` is accepted.
    #[arg(long = "format", value_parser = ["json"])]
    pub format: String,

    /// Path to one existing health JSON document or evidence bundle.
    #[arg(long = "input")]
    pub input: PathBuf,
}
