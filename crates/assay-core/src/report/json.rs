use crate::report::RunArtifacts;
use std::path::Path;

/// Render the run results report as a pretty-printed JSON string.
///
/// Single source of truth for the JSON report shape: both the file writer
/// ([`write_json`]) and stdout emission (`assay run --format json`) use it,
/// so the on-disk and piped representations never diverge.
pub fn render_json(artifacts: &RunArtifacts) -> anyhow::Result<String> {
    let v = serde_json::json!({
        "run_id": artifacts.run_id,
        "suite": artifacts.suite,
        "results": artifacts.results,
    });
    Ok(serde_json::to_string_pretty(&v)?)
}

pub fn write_json(artifacts: &RunArtifacts, out: &Path) -> anyhow::Result<()> {
    std::fs::write(out, render_json(artifacts)?)?;
    Ok(())
}
