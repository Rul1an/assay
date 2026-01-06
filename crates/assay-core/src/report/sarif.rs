use crate::model::{TestResultRow, TestStatus};
use std::path::Path;

pub fn write_sarif(tool_name: &str, results: &[TestResultRow], out: &Path) -> anyhow::Result<()> {
    let sarif_results: Vec<serde_json::Value> = results
        .iter()
        .filter_map(|r| {
            let level = match r.status {
                TestStatus::Pass | TestStatus::Skipped | TestStatus::AllowedOnError => return None,
                TestStatus::Warn | TestStatus::Flaky | TestStatus::Unstable => "warning",
                TestStatus::Fail | TestStatus::Error => "error",
            };
            Some(serde_json::json!({
                "ruleId": "assay",
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

pub fn build_sarif_diagnostics(
    tool_name: &str,
    diagnostics: &[crate::errors::diagnostic::Diagnostic],
) -> serde_json::Value {
    let sarif_results: Vec<serde_json::Value> = diagnostics
        .iter()
        .map(|d| {
            let level = match d.severity.as_str() {
                "error" => "error",
                "warn" => "warning",
                _ => "note",
            };

            // Map code to ruleId (use simple code string for now)
            let rule_id = &d.code;

            // Optional: location (if context provides file/line)
            let locations = if let Some(file) = d.context.get("file").and_then(|v| v.as_str()) {
                vec![serde_json::json!({
                    "physicalLocation": {
                        "artifactLocation": { "uri": file }
                    }
                })]
            } else {
                vec![]
            };

            serde_json::json!({
                "ruleId": rule_id,
                "level": level,
                "message": { "text": d.message },
                "locations": locations
            })
        })
        .collect();

    serde_json::json!({
        "version": "2.1.0",
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "runs": [{
            "tool": {
                "driver": {
                    "name": tool_name,
                    "version": env!("CARGO_PKG_VERSION")
                }
            },
            "results": sarif_results,
            "invocations": [{
                "executionSuccessful": diagnostics.iter().all(|d| {
                    !matches!(d.severity.as_str(), "error" | "ERROR")
                }),
                "exitCode": 0 // Actual CLI exit code depends on policy, but tool invocation is "complete"
            }]
        }]
    })
}

pub fn write_sarif_diagnostics(
    tool_name: &str,
    diagnostics: &[crate::errors::diagnostic::Diagnostic],
    out: &Path,
) -> anyhow::Result<()> {
    let doc = build_sarif_diagnostics(tool_name, diagnostics);
    std::fs::write(out, serde_json::to_string_pretty(&doc)?)?;
    Ok(())
}
