use crate::render_safety::{render_details_safe, Sink};
use crate::report::RunArtifacts;
use std::path::Path;

/// Render the run results report as a pretty-printed JSON string.
///
/// Single source of truth for the JSON report shape: both the file writer
/// ([`write_json`]) and stdout emission (`assay run --format json`) use it,
/// so the on-disk and piped representations never diverge.
///
/// Render-safety (MCP01a): the untrusted model / agent / tool content carried in result `message`
/// and `details.*` is rendered through the render-safety pipeline before serialization, so a raw
/// credential / PII / terminal-control value never reaches `run.json`. As a record sink it redacts
/// and control-strips but does NOT truncate (`usize::MAX`): the eval record keeps full, redacted
/// content. Assay-owned keys (ids, status, score, fingerprint, skip.*) stay byte-stable.
pub fn render_json(artifacts: &RunArtifacts) -> anyhow::Result<String> {
    let v = serde_json::json!({
        "run_id": artifacts.run_id,
        "suite": artifacts.suite,
        "results": artifacts.results,
    });
    let safe = render_details_safe(Sink::Json, &v, usize::MAX);
    Ok(serde_json::to_string_pretty(&safe)?)
}

pub fn write_json(artifacts: &RunArtifacts, out: &Path) -> anyhow::Result<()> {
    std::fs::write(out, render_json(artifacts)?)?;
    Ok(())
}
