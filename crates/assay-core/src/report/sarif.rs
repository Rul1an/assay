use crate::model::{TestResultRow, TestStatus};
use std::path::Path;

/// Default maximum number of results to include in SARIF output.
/// GitHub Code Scanning accepts up to 25_000 results per run (soft limit); exceeding causes upload issues.
pub const DEFAULT_SARIF_MAX_RESULTS: usize = 25_000;

/// Outcome of writing SARIF: when truncation was applied, how many results were omitted.
#[derive(Debug, Clone, Default)]
pub struct SarifWriteOutcome {
    /// Number of results omitted due to max_results limit (0 when no truncation).
    pub omitted_count: u64,
}

/// SARIF schema version used by all Assay SARIF producers.
///
/// Shared contract with `assay-evidence::lint::sarif` — both modules MUST use
/// the same schema URI and version `"2.1.0"`.  When changing this constant,
/// update the sibling in `assay-evidence/src/lint/sarif.rs` as well.
pub const SARIF_SCHEMA: &str =
    "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/main/sarif-2.1/schema/sarif-schema-2.1.0.json";

/// Synthetic location for results without file context.
///
/// GitHub Code Scanning requires at least one location per result, so we use
/// a synthetic fallback pointing to the config file when no real file is available.
const SYNTHETIC_LOCATION_URI: &str = ".assay/eval.yaml";

/// Whether this status is included in SARIF output (deterministic truncation contract).
/// Eligible: Fail, Error, Warn, Flaky, Unstable. Excluded: Pass, Skipped, AllowedOnError.
#[inline]
pub fn is_sarif_eligible(status: TestStatus) -> bool {
    !matches!(
        status,
        TestStatus::Pass | TestStatus::Skipped | TestStatus::AllowedOnError
    )
}

/// Blocking rank for truncation order: 0 = blocking (Fail/Error), 1 = non-blocking (Warn/Flaky/Unstable).
/// Policy-proof: E7.4 suite policy can change what "blocks" CI without changing this rank.
#[inline]
pub fn blocking_rank(status: TestStatus) -> u8 {
    if status.is_blocking() {
        0
    } else {
        1
    }
}

/// Severity rank for SARIF truncation: 0 = error, 1 = warning, 2 = note.
/// The `_ => 2` branch is reserved for future eligible severities (e.g. note-level); currently all eligible statuses map to 0 or 1.
#[inline]
pub fn severity_rank(status: TestStatus) -> u8 {
    match status {
        TestStatus::Fail | TestStatus::Error => 0,
        TestStatus::Warn | TestStatus::Flaky | TestStatus::Unstable => 1,
        _ => 2,
    }
}

/// Sort key for deterministic truncation: (BlockingRank, SeverityRank, test_id). Stable and input-order independent.
fn sarif_sort_key(r: &TestResultRow) -> (u8, u8, &str) {
    (
        blocking_rank(r.status),
        severity_rank(r.status),
        r.test_id.as_str(),
    )
}

/// Writes test results as SARIF 2.1.0 with an explicit result limit. Use this when you need a custom
/// limit (e.g. contract tests with a small limit). For production, prefer [`write_sarif`] which uses
/// [`DEFAULT_SARIF_MAX_RESULTS`].
///
/// Truncation is deterministic: filter to eligible results → sort by (BlockingRank, SeverityRank, test_id) → take first `max_results`.
/// `omitted_count` = eligible_total - included (only eligible results are counted).
pub fn write_sarif_with_limit(
    tool_name: &str,
    results: &[TestResultRow],
    out: &Path,
    max_results: usize,
) -> anyhow::Result<SarifWriteOutcome> {
    let eligible: Vec<&TestResultRow> = results
        .iter()
        .filter(|r| is_sarif_eligible(r.status))
        .collect();
    let eligible_total = eligible.len();

    let mut sorted: Vec<&TestResultRow> = eligible;
    sorted.sort_by_cached_key(|r| sarif_sort_key(r));
    let kept: Vec<&TestResultRow> = sorted.into_iter().take(max_results).collect();
    let kept_count = kept.len();
    let omitted_count = eligible_total.saturating_sub(kept_count) as u64;

    let sarif_results: Vec<serde_json::Value> = kept
        .iter()
        .map(|r| {
            let level = match r.status {
                TestStatus::Warn | TestStatus::Flaky | TestStatus::Unstable => "warning",
                TestStatus::Fail | TestStatus::Error => "error",
                _ => "note",
            };
            serde_json::json!({
                "ruleId": "assay",
                "level": level,
                "message": { "text": format!("{}: {}", r.test_id, r.message) },
                "locations": [{
                    "physicalLocation": {
                        "artifactLocation": { "uri": SYNTHETIC_LOCATION_URI },
                        "region": { "startLine": 1, "startColumn": 1 }
                    }
                }]
            })
        })
        .collect();

    let run_obj: serde_json::Value = if omitted_count > 0 {
        serde_json::json!({
            "tool": { "driver": { "name": tool_name } },
            "results": sarif_results,
            "properties": {
                "assay": {
                    "truncated": true,
                    "omitted_count": omitted_count
                }
            }
        })
    } else {
        serde_json::json!({
            "tool": { "driver": { "name": tool_name } },
            "results": sarif_results
        })
    };

    let doc = serde_json::json!({
        "version": "2.1.0",
        "$schema": SARIF_SCHEMA,
        "runs": [run_obj]
    });

    std::fs::write(out, serde_json::to_string_pretty(&doc)?)?;
    Ok(SarifWriteOutcome { omitted_count })
}

/// Write test results as SARIF 2.1.0 to a file using the default result limit ([`DEFAULT_SARIF_MAX_RESULTS`]).
///
/// For custom limits (e.g. tests), use [`write_sarif_with_limit`]. Truncation is deterministic:
/// eligible results (Fail/Error/Warn/Flaky/Unstable) are sorted by (BlockingRank, SeverityRank, test_id)
/// then truncated; run-level `runs[0].properties.assay` holds truncated/omitted_count when applicable.
///
/// # SARIF consistency contract
///
/// There are two SARIF producers in the Assay workspace:
///
/// | Producer | Crate | Purpose |
/// |----------|-------|---------|
/// | `write_sarif` / `write_sarif_with_limit` / `build_sarif_diagnostics` (this module) | `assay-core` | Test results & diagnostic reports |
/// | `to_sarif` | `assay-evidence` | Evidence-bundle lint findings for GitHub Code Scanning |
///
/// **Shared invariants** (must stay in sync):
/// - SARIF version: `"2.1.0"`
/// - Schema URI: [`SARIF_SCHEMA`]
/// - Severity mapping: `Error`→`"error"`, `Warn`→`"warning"`, `Info`/other→`"note"`
pub fn write_sarif(
    tool_name: &str,
    results: &[TestResultRow],
    out: &Path,
) -> anyhow::Result<SarifWriteOutcome> {
    write_sarif_with_limit(tool_name, results, out, DEFAULT_SARIF_MAX_RESULTS)
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

            // Always include at least one location; use context file or synthetic fallback
            let file_uri = d
                .context
                .get("file")
                .and_then(|v| v.as_str())
                .unwrap_or(SYNTHETIC_LOCATION_URI);
            let line = d.context.get("line").and_then(|v| v.as_u64()).unwrap_or(1);
            let locations = vec![serde_json::json!({
                "physicalLocation": {
                    "artifactLocation": { "uri": file_uri },
                    "region": { "startLine": line, "startColumn": 1 }
                }
            })];

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
        "$schema": SARIF_SCHEMA,
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

    /// Contract test: every SARIF result MUST have at least one location
    /// (required for GitHub Code Scanning to accept the file)
    #[test]
    fn test_sarif_location_invariant_with_context() {
        let diag = Diagnostic::new("TEST002", "Error with file context".to_string())
            .with_severity("error")
            .with_context(json!({"file": "src/main.rs", "line": 42}));

        let sarif = build_sarif_diagnostics("assay", &[diag], 1);
        let results = sarif["runs"][0]["results"].as_array().unwrap();

        // Must have at least one location
        let locations = results[0]["locations"].as_array().unwrap();
        assert!(
            !locations.is_empty(),
            "SARIF result must have at least one location"
        );

        // Should use the provided file
        let uri = &locations[0]["physicalLocation"]["artifactLocation"]["uri"];
        assert_eq!(uri, "src/main.rs");

        // Should include line number
        let line = &locations[0]["physicalLocation"]["region"]["startLine"];
        assert_eq!(line, 42);
    }

    /// Contract test: SARIF results without file context get synthetic location
    #[test]
    fn test_sarif_location_invariant_synthetic_fallback() {
        let diag = Diagnostic::new("TEST003", "Error without file context".to_string())
            .with_severity("error");
        // No context set - should use synthetic location

        let sarif = build_sarif_diagnostics("assay", &[diag], 1);
        let results = sarif["runs"][0]["results"].as_array().unwrap();

        // Must have at least one location (synthetic)
        let locations = results[0]["locations"].as_array().unwrap();
        assert!(
            !locations.is_empty(),
            "SARIF result must have synthetic location fallback"
        );

        // Should use the synthetic location URI
        let uri = &locations[0]["physicalLocation"]["artifactLocation"]["uri"];
        assert_eq!(uri, SYNTHETIC_LOCATION_URI);
    }
}
