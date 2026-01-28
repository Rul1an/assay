use super::packs::executor::PackExecutionMeta;
use super::rules::RULES;
use super::LintReport;
use serde_json::json;

/// SARIF schema version used by all Assay SARIF producers.
///
/// Shared contract with `assay-core::report::sarif` — both modules MUST use the
/// same schema URI and version `"2.1.0"`.  When changing this constant, update
/// the sibling in `assay-core/src/report/sarif.rs` as well.
pub const SARIF_SCHEMA: &str =
    "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/main/sarif-2.1/schema/sarif-schema-2.1.0.json";

/// SARIF output options.
#[derive(Debug, Clone, Default)]
pub struct SarifOptions {
    /// Pack execution metadata (for pack-enhanced SARIF).
    pub pack_meta: Option<PackExecutionMeta>,
    /// Bundle path for locations (default: "bundle.tar.gz").
    pub bundle_path: Option<String>,
    /// Working directory for invocations.
    pub working_directory: Option<String>,
}

/// Convert a LintReport to SARIF 2.1.0 format.
///
/// # SARIF consistency contract
///
/// There are two SARIF producers in the Assay workspace:
///
/// | Producer | Crate | Purpose |
/// |----------|-------|---------|
/// | `to_sarif` (this fn) | `assay-evidence` | Evidence-bundle lint findings for GitHub Code Scanning |
/// | `write_sarif` / `build_sarif_diagnostics` | `assay-core` | Test results & diagnostic reports |
///
/// **Shared invariants** (must stay in sync):
/// - SARIF version: `"2.1.0"`
/// - Schema URI: [`SARIF_SCHEMA`]
/// - Severity mapping: `Error`→`"error"`, `Warn`→`"warning"`, `Info`/other→`"note"`
///
/// **Intentional differences** (by design, not drift):
/// - This producer includes `partialFingerprints` and `automationDetails` for
///   GitHub Code Scanning deduplication; `assay-core` does not.
/// - This producer populates `tool.driver.rules[]` from the lint rule registry;
///   `assay-core` uses a single generic `ruleId`.
/// - `assay-core` includes `invocations[]` with exit codes; this producer does not.
pub fn to_sarif(report: &LintReport) -> serde_json::Value {
    to_sarif_with_options(report, SarifOptions::default())
}

/// Convert a LintReport to SARIF 2.1.0 format with options.
///
/// This enhanced version supports:
/// - Pack metadata in tool.driver.properties.assayPacks
/// - locations[] on all results (including global findings)
/// - primaryLocationLineHash for GitHub deduplication
/// - run.properties.disclaimer for compliance packs
/// - invocations with workingDirectory
pub fn to_sarif_with_options(report: &LintReport, options: SarifOptions) -> serde_json::Value {
    let bundle_path = options.bundle_path.as_deref().unwrap_or("bundle.tar.gz");

    // Build rules from built-in registry + pack rules
    let mut rules: Vec<serde_json::Value> = RULES
        .iter()
        .map(|r| {
            let mut rule = json!({
                "id": r.id,
                "shortDescription": {
                    "text": r.description
                },
                "defaultConfiguration": {
                    "level": severity_to_sarif_level(&r.default_severity)
                }
            });

            if let Some(uri) = r.help_uri {
                rule.as_object_mut()
                    .unwrap()
                    .insert("helpUri".into(), serde_json::Value::String(uri.into()));
            }

            if !r.tags.is_empty() || r.security_severity.is_some() {
                let mut props = serde_json::Map::new();
                if !r.tags.is_empty() {
                    props.insert("tags".into(), json!(r.tags));
                }
                if let Some(ss) = r.security_severity {
                    props.insert("security-severity".into(), json!(ss));
                }
                rule.as_object_mut()
                    .unwrap()
                    .insert("properties".into(), serde_json::Value::Object(props));
            }

            rule
        })
        .collect();

    // Add pack rules if packs are present
    if options.pack_meta.is_some() {
        // Extract unique pack rules from findings
        let mut pack_rule_ids = std::collections::HashSet::new();
        for finding in &report.findings {
            if finding.rule_id.contains('@') {
                // Pack rule (canonical format)
                pack_rule_ids.insert(finding.rule_id.clone());
            }
        }

        // Sort for deterministic SARIF output
        let mut pack_rule_ids: Vec<String> = pack_rule_ids.into_iter().collect();
        pack_rule_ids.sort();

        for rule_id in pack_rule_ids {
            // Extract pack info from tags
            let short_id = extract_tag(&report.findings, &rule_id, "short_id:");
            let article_ref = extract_tag(&report.findings, &rule_id, "article_ref:");
            let pack_name = rule_id.split('@').next().unwrap_or("");
            let pack_version = rule_id
                .split('@')
                .nth(1)
                .and_then(|s| s.split(':').next())
                .unwrap_or("");

            let mut props = serde_json::Map::new();
            props.insert("pack".into(), json!(pack_name));
            props.insert("pack_version".into(), json!(pack_version));
            if let Some(sid) = &short_id {
                props.insert("short_id".into(), json!(sid));
            }
            if let Some(aref) = &article_ref {
                props.insert("article_ref".into(), json!(aref));
            }

            rules.push(json!({
                "id": rule_id,
                "shortDescription": {
                    "text": format!("Pack rule {}", short_id.as_deref().unwrap_or(&rule_id))
                },
                "defaultConfiguration": {
                    "level": "error"
                },
                "properties": props
            }));
        }
    }

    // Build results with enhanced locations
    let results: Vec<serde_json::Value> = report
        .findings
        .iter()
        .map(|f| {
            // Determine artifact URI and line
            let (artifact_uri, start_line) = match &f.location {
                Some(loc) => ("events.ndjson".to_string(), loc.line),
                None => (bundle_path.to_string(), 1),
            };

            // Extract primaryLocationLineHash from tags if present
            let primary_hash = f
                .tags
                .iter()
                .find(|t| t.starts_with("primaryLocationLineHash:"))
                .and_then(|t| t.strip_prefix("primaryLocationLineHash:"))
                .map(|s| s.to_string());

            let mut partial_fingerprints = serde_json::Map::new();
            partial_fingerprints.insert("assayLintFingerprint/v1".into(), json!(f.fingerprint));
            if let Some(ph) = primary_hash {
                partial_fingerprints.insert("primaryLocationLineHash".into(), json!(ph));
            }

            // Build location (always present for GitHub)
            let location = json!({
                "physicalLocation": {
                    "artifactLocation": {
                        "uri": artifact_uri,
                        "uriBaseId": "%SRCROOT%"
                    },
                    "region": {
                        "startLine": start_line,
                        "startColumn": 1
                    }
                }
            });

            // Build result properties
            let mut result_props = serde_json::Map::new();
            if !f.tags.is_empty() {
                // Filter out internal metadata tags
                let visible_tags: Vec<&str> = f
                    .tags
                    .iter()
                    .filter(|t| {
                        !t.starts_with("primaryLocationLineHash:")
                            && !t.starts_with("pack_version:")
                            && !t.starts_with("short_id:")
                    })
                    .map(|s| s.as_str())
                    .collect();
                if !visible_tags.is_empty() {
                    result_props.insert("tags".into(), json!(visible_tags));
                }
            }

            // Add article_ref to properties
            if let Some(aref) = f.tags.iter().find(|t| t.starts_with("article_ref:")) {
                if let Some(ref_value) = aref.strip_prefix("article_ref:") {
                    result_props.insert("article_ref".into(), json!(ref_value));
                }
            }

            let mut result = json!({
                "ruleId": f.rule_id,
                "level": f.severity.as_sarif_level(),
                "message": {
                    "text": f.message
                },
                "locations": [location],
                "partialFingerprints": partial_fingerprints
            });

            if !result_props.is_empty() {
                result
                    .as_object_mut()
                    .unwrap()
                    .insert("properties".into(), serde_json::Value::Object(result_props));
            }

            // Add logical location for event-specific findings
            if let Some(loc) = &f.location {
                result.as_object_mut().unwrap()["locations"]
                    .as_array_mut()
                    .unwrap()[0]
                    .as_object_mut()
                    .unwrap()
                    .insert(
                        "logicalLocations".into(),
                        json!([{
                            "name": format!("seq:{}", loc.seq),
                            "kind": "event"
                        }]),
                    );
            }

            result
        })
        .collect();

    // Build tool.driver.properties for packs
    let mut driver_props = serde_json::Map::new();
    if let Some(ref meta) = options.pack_meta {
        let assay_packs: Vec<serde_json::Value> = meta
            .packs
            .iter()
            .map(|p| {
                json!({
                    "name": p.name,
                    "version": p.version,
                    "digest": p.digest,
                    "source_url": p.source_url
                })
            })
            .collect();
        driver_props.insert("assayPacks".into(), json!(assay_packs));
    }

    // Build run.properties
    let mut run_props = serde_json::Map::new();
    if let Some(ref meta) = options.pack_meta {
        if let Some(ref disclaimer) = meta.disclaimer {
            run_props.insert("disclaimer".into(), json!(disclaimer));
        }
        if meta.truncated {
            run_props.insert("truncated".into(), json!(true));
            run_props.insert("truncatedCount".into(), json!(meta.truncated_count));
        }
    }

    let automation_id = format!(
        "assay-evidence/lint/{}/{}",
        report.bundle_meta.run_id, report.tool_version
    );

    // Build invocations
    let mut invocation = json!({
        "executionSuccessful": true
    });
    if let Some(ref wd) = options.working_directory {
        // Construct proper file:// URI using url crate
        let wd_path = std::path::Path::new(wd);
        if let Ok(url) = url::Url::from_directory_path(wd_path) {
            invocation
                .as_object_mut()
                .unwrap()
                .insert("workingDirectory".into(), json!({ "uri": url.as_str() }));
        }
    }

    // Build tool.driver
    let mut driver = json!({
        "name": "assay-evidence-lint",
        "version": report.tool_version,
        "semanticVersion": report.tool_version,
        "informationUri": "https://docs.assay.dev/lint",
        "rules": rules
    });
    if !driver_props.is_empty() {
        driver
            .as_object_mut()
            .unwrap()
            .insert("properties".into(), serde_json::Value::Object(driver_props));
    }

    // Build run
    let mut run = json!({
        "tool": {
            "driver": driver
        },
        "invocations": [invocation],
        "automationDetails": {
            "id": automation_id,
            "description": {
                "text": format!("Lint results for bundle {}", report.bundle_meta.run_id)
            }
        },
        "results": results
    });
    if !run_props.is_empty() {
        run.as_object_mut()
            .unwrap()
            .insert("properties".into(), serde_json::Value::Object(run_props));
    }

    json!({
        "$schema": SARIF_SCHEMA,
        "version": "2.1.0",
        "runs": [run]
    })
}

/// Extract a tag value from findings for a specific rule.
fn extract_tag(findings: &[super::LintFinding], rule_id: &str, prefix: &str) -> Option<String> {
    findings
        .iter()
        .find(|f| f.rule_id == rule_id)
        .and_then(|f| {
            f.tags
                .iter()
                .find(|t| t.starts_with(prefix))
                .and_then(|t| t.strip_prefix(prefix))
                .map(|s| s.to_string())
        })
}

fn severity_to_sarif_level(severity: &super::Severity) -> &'static str {
    match severity {
        super::Severity::Error => "error",
        super::Severity::Warn => "warning",
        super::Severity::Info => "note",
    }
}
