use super::rules::RULES;
use super::LintReport;
use serde_json::json;

/// Convert a LintReport to SARIF 2.1.0 format.
pub fn to_sarif(report: &LintReport) -> serde_json::Value {
    let rules: Vec<serde_json::Value> = RULES
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

    let results: Vec<serde_json::Value> = report
        .findings
        .iter()
        .map(|f| {
            let mut result = json!({
                "ruleId": f.rule_id,
                "level": f.severity.as_sarif_level(),
                "message": {
                    "text": f.message
                },
                "partialFingerprints": {
                    "assayLintFingerprint/v1": f.fingerprint
                }
            });

            if let Some(loc) = &f.location {
                result.as_object_mut().unwrap().insert(
                    "locations".into(),
                    json!([{
                        "physicalLocation": {
                            "artifactLocation": {
                                "uri": "events.ndjson"
                            },
                            "region": {
                                "startLine": loc.line
                            }
                        },
                        "logicalLocations": [{
                            "name": format!("seq:{}", loc.seq),
                            "kind": "event"
                        }]
                    }]),
                );
            }

            if !f.tags.is_empty() {
                result
                    .as_object_mut()
                    .unwrap()
                    .insert("properties".into(), json!({ "tags": f.tags }));
            }

            result
        })
        .collect();

    // automationDetails.id includes run_id to distinguish uploads per bundle.
    // Fingerprints are stable within a bundle (rule_id + event position), so same
    // findings in the same bundle across reruns will be deduped by GitHub.
    // Different bundles get different automation IDs, preventing cross-bundle dedup.
    let automation_id = format!(
        "assay-evidence/lint/{}/{}",
        report.bundle_meta.run_id, report.tool_version
    );

    json!({
        "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/main/sarif-2.1/schema/sarif-schema-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "assay-evidence-lint",
                    "version": report.tool_version,
                    "informationUri": "https://docs.assay.dev/lint",
                    "rules": rules
                }
            },
            "automationDetails": {
                "id": automation_id,
                "description": {
                    "text": format!("Lint results for bundle {}", report.bundle_meta.run_id)
                }
            },
            "results": results
        }]
    })
}

fn severity_to_sarif_level(severity: &super::Severity) -> &'static str {
    match severity {
        super::Severity::Error => "error",
        super::Severity::Warn => "warning",
        super::Severity::Info => "note",
    }
}
