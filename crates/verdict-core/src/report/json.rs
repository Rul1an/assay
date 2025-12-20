use crate::report::RunArtifacts;
use std::path::Path;

pub fn write_json(artifacts: &RunArtifacts, out: &Path) -> anyhow::Result<()> {
    let v = serde_json::json!({
        "run_id": artifacts.run_id,
        "suite": artifacts.suite,
        "results": artifacts.results,
    });
    std::fs::write(out, serde_json::to_string_pretty(&v)?)?;
    Ok(())
}
