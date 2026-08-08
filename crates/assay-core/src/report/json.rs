use crate::render_safety::{render_details_safe, Sink};
use crate::report::RunArtifacts;
use std::path::Path;

pub const RUN_REPORT_SCHEMA: &str = "assay.run_report.v1";
pub const RUN_REPORT_SCHEMA_VERSION: u32 = 1;

/// Render the run results report as a pretty-printed JSON string.
///
/// Single source of truth for the detailed run-results report emitted by
/// `assay run --format json` and by the [`write_json`] helper. The CLI's
/// extended `run.json` artifact is a separate envelope and writer.
///
/// Render-safety (MCP01a): the untrusted model / agent / tool content carried in result `message`
/// and `details.*` is rendered through the render-safety pipeline before serialization, so a raw
/// credential / PII / terminal-control value never reaches this rendered report. As a record sink
/// it redacts and control-strips but does NOT truncate (`usize::MAX`): the eval record keeps full,
/// redacted content. Assay-owned keys (ids, status, score, fingerprint, skip.*) stay byte-stable.
pub fn render_json(artifacts: &RunArtifacts) -> anyhow::Result<String> {
    let v = serde_json::json!({
        "schema": RUN_REPORT_SCHEMA,
        "schema_version": RUN_REPORT_SCHEMA_VERSION,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::RunArtifacts;

    #[test]
    fn rendered_run_report_has_named_integer_versioned_schema() {
        let artifacts = RunArtifacts {
            run_id: 1,
            suite: "schema-contract".to_string(),
            results: Vec::new(),
            order_seed: None,
            runner_clone_ms: None,
        };

        let rendered: serde_json::Value =
            serde_json::from_str(&render_json(&artifacts).expect("render run report"))
                .expect("parse run report");

        assert_eq!(RUN_REPORT_SCHEMA, "assay.run_report.v1");
        assert_eq!(RUN_REPORT_SCHEMA_VERSION, 1);
        assert_eq!(rendered["schema"], RUN_REPORT_SCHEMA);
        assert_eq!(rendered["schema_version"], RUN_REPORT_SCHEMA_VERSION);
    }
}
