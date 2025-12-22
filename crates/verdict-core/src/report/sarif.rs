use crate::model::{TestResultRow, TestStatus};
use std::path::Path;

pub fn write_sarif(tool_name: &str, results: &[TestResultRow], out: &Path) -> anyhow::Result<()> {
    let sarif_results: Vec<serde_json::Value> = results
        .iter()
        .filter_map(|r| {
            let level = match r.status {
                TestStatus::Pass | TestStatus::Skipped => return None,
                TestStatus::Warn | TestStatus::Flaky | TestStatus::Unstable => "warning",
                TestStatus::Fail | TestStatus::Error => "error",
            };
            Some(serde_json::json!({
                "ruleId": "verdict",
                "level": level,
                "message": { "text": format!("{}: {}", r.test_id, r.message) },
            }))
        })
        .collect();

    let doc = serde_json::json!({
      "version": "2.1.0",
      "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
      "runs": [{
        "tool": { "driver": { "name": tool_name } },
        "results": sarif_results
      }]
    });

    std::fs::write(out, serde_json::to_string_pretty(&doc)?)?;
    Ok(())
}
