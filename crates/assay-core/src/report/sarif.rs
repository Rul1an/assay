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
    exit_code: i32,
) -> serde_json::Value {
    fn normalize_severity(s: &str) -> &str {
        match s {
            "error" | "ERROR" => "error",
            "warn" | "warning" | "WARN" | "WARNING" => "warning",
            _ => "note",
        }
    }

    let sarif_results: Vec<serde_json::Value> = diagnostics
        .iter()
        .map(|d| {
            let level = normalize_severity(d.severity.as_str());

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

    let execution_successful = !diagnostics.iter().any(|d| {
        let s = normalize_severity(d.severity.as_str());
        s == "error"
    });

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
                "executionSuccessful": execution_successful,
                "exitCode": exit_code
            }]
        }]
    })
}

pub fn write_sarif_diagnostics(
    tool_name: &str,
    diagnostics: &[crate::errors::diagnostic::Diagnostic],
    out: &std::path::Path,
) -> anyhow::Result<()> {
    // For file dump, we assume exit code 0 or generic?
    // Actually, maybe we should let the caller pass it if they care.
    // For now, defaulting to 0 is safe for just "writing diagnostics".
    let doc = build_sarif_diagnostics(tool_name, diagnostics, 0);
    std::fs::write(out, serde_json::to_string_pretty(&doc)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::diagnostic::Diagnostic;
    use serde_json::json;

    #[test]
    fn test_sarif_generation() {
        let diag = Diagnostic::new("TEST001", "Test error".to_string())
            .with_severity("error")
            .with_context(json!({"file": "test.rs"}));

        let sarif = build_sarif_diagnostics("assay-test", &[diag], 1);

        let runs = sarif["runs"].as_array().unwrap();
        assert_eq!(runs.len(), 1);

        let results = runs[0]["results"].as_array().unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["level"], "error");
        assert_eq!(results[0]["ruleId"], "TEST001");

        let invocations = runs[0]["invocations"].as_array().unwrap();
        assert!(!invocations[0]["executionSuccessful"].as_bool().unwrap());
        assert_eq!(invocations[0]["exitCode"], 1);
    }
}
