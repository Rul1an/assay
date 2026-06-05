use std::path::Path;

use super::types::Summary;

/// Write summary.json to file.
pub fn write_summary(summary: &Summary, out: &Path) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(summary)?;
    std::fs::write(out, json)?;
    Ok(())
}
