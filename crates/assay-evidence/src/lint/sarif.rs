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
    /// Deprecated: Working directory is no longer included in SARIF output
    /// to avoid leaking local filesystem paths. This field is ignored.
    #[deprecated(note = "workingDirectory is no longer included in SARIF to avoid path leakage")]
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
                    "level": r.default_severity.as_sarif_level()
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
    if let Some(ref meta) = options.pack_meta {
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

            // Get rule metadata for fullDescription and help.markdown
            let rule_meta = meta.rule_metadata.get(&rule_id);

            let short_desc = rule_meta
                .map(|m| m.description.as_str())
                .unwrap_or_else(|| short_id.as_deref().unwrap_or(&rule_id));

            let mut rule = json!({
                "id": rule_id,
                "shortDescription": {
                    "text": short_desc
                },
                "defaultConfiguration": {
                    "level": "error"
                },
                "properties": props
            });

            // Add fullDescription (required for GitHub "rule help")
            if let Some(meta) = rule_meta {
                rule.as_object_mut().unwrap().insert(
                    "fullDescription".into(),
                    json!({ "text": meta.full_description }),
                );

                // Add help.markdown (shown in GitHub alert details)
                rule.as_object_mut().unwrap().insert(
                    "help".into(),
                    json!({ "markdown": meta.help_markdown, "text": meta.description }),
                );

                // Add helpUri if available
                if let Some(ref uri) = meta.help_uri {
                    rule.as_object_mut()
                        .unwrap()
                        .insert("helpUri".into(), json!(uri));
                }
            }

            rules.push(rule);
        }
    }

    // Determine anchor file for global findings (repo-relative)
    let anchor_file = options
        .pack_meta
        .as_ref()
        .and_then(|m| m.anchor_file.as_deref())
        .unwrap_or(bundle_path);

    // Build results with enhanced locations
    let results: Vec<serde_json::Value> = report
        .findings
        .iter()
        .map(|f| {
            // Determine artifact URI and line
            // For global findings (no location), use anchor file in repo
            // For event-specific findings, use events.ndjson
            let (artifact_uri, start_line) = match &f.location {
                Some(loc) => ("events.ndjson".to_string(), loc.line),
                None => (anchor_file.to_string(), 1),
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
            // Use repo-relative URI without uriBaseId for simplicity
            let location = json!({
                "physicalLocation": {
                    "artifactLocation": {
                        "uri": artifact_uri
                    },
                    "region": {
                        "startLine": start_line,
                        "startColumn": 1
                    }
                }
            });

            // Build result properties
            let mut result_props = serde_json::Map::new();

            // Add bundle_path and bundle_id for traceability (not as location)
            if let Some(ref meta) = options.pack_meta {
                if let Some(ref bp) = meta.bundle_path {
                    result_props.insert("bundle_path".into(), json!(bp));
                }
                if let Some(ref bid) = meta.bundle_id {
                    result_props.insert("bundle_id".into(), json!(bid));
                }
            }

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
    //
    // Truncation is read off the report rather than the pack metadata. Both carry it and both are
    // set from the same values, but pack metadata is absent whenever no packs are configured, so
    // keying on it meant a default-path run disclosed nothing here at all.
    let mut run_props = serde_json::Map::new();
    if let Some(ref meta) = options.pack_meta {
        if let Some(ref disclaimer) = meta.disclaimer {
            run_props.insert("disclaimer".into(), json!(disclaimer));
        }
    }
    if report.truncated {
        run_props.append(&mut truncation_properties(report));
    }

    let automation_id = format!(
        "assay-evidence/lint/{}/{}",
        report.bundle_meta.run_id, report.tool_version
    );

    // Build invocations
    // Note: workingDirectory is intentionally omitted to avoid leaking local paths
    // (e.g., /Users/... or /home/...). GitHub Code Scanning doesn't require it.
    //
    // `executionSuccessful` stays true on a truncated run. SARIF 2.1.0 section 3.20.14 defines it
    // as true "if the engineering system that started the process knows that the analysis tool
    // succeeded", and its own example pairs `executionSuccessful: true` with a non-zero exit code.
    // A capped run succeeded; it reported less than it found. Flipping this would assert a failure
    // that did not happen.
    //
    // The cap itself is the deeper problem, and it is worth stating here rather than only in a
    // commit message. Section 3.14.23 is normative: outside the failed-to-start cases, `results`
    // "SHALL be present and SHALL contain all results detected by the tool". A configured
    // `max_results` therefore puts this producer out of conformance whenever it fires. The cap
    // exists because downstream consumers impose upload limits, so this is a real tension and not
    // an oversight, but it is a tension the format resolves against us. An earlier version of this
    // comment claimed SARIF had no home for a reporting cap; that was wrong, and the correction is
    // that SARIF has a position instead of a gap.
    //
    // Until the cap goes, disclose it in both available places and claim neither as a blessing.
    // `run.properties` is the machine-readable carrier. The notification is a human-readable aid at
    // `warning`, which 3.58.6 defines as covering the case where "the analysis might be incomplete
    // but the results that were generated are probably valid". It stays below Appendix I's `error`
    // gate deliberately: 3.20.21 makes an error-level notification mean the run failed, and a cap
    // is not a failed run.
    let mut invocation = json!({
        "executionSuccessful": true
    });
    if report.truncated {
        invocation.as_object_mut().unwrap().insert(
            "toolExecutionNotifications".into(),
            json!([{
                "descriptor": { "id": "ASSAY-LINT-TRUNCATED" },
                "level": "warning",
                "message": {
                    "text": format!(
                        "Result set is incomplete: {} finding(s) were dropped by a max_results \
                         cap of {}. The findings reported here are the highest severity that \
                         survived the cap; absence of a lower-severity finding does not mean it \
                         was absent from the bundle.",
                        report.truncated_count, report.applied_cap
                    )
                },
                // The same keys the run properties carry, from the same builder — see
                // `truncation_properties`.
                "properties": truncation_properties(report)
            }]),
        );
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

/// The truncation disclosure as one set of keys, built once for both carriers.
///
/// `run.properties` and the tool-execution notification describe the same cap firing, so they say
/// it the same way. They used to disagree: the notification called the suppressed count
/// `droppedCount` while the run called it `truncatedCount`, which left a consumer reading two names
/// for one number with no way to tell whether the difference was a difference in meaning.
/// `truncatedCount` is the name that survives because it is the published one — the pack-engine
/// spec and the changelog both name `run.properties.truncated` / `truncatedCount`, and nothing
/// documents `droppedCount`.
///
/// `appliedCap` travels with the count rather than only near it: without the ceiling a consumer
/// sees how many findings fell past a bound but not what the bound was (OWASP agentic-skills #49
/// review point). `truncated` is redundant inside a notification that is only emitted on a
/// truncated run, and it is emitted anyway, because a consumer that reads one bag should not have
/// to know which bag it is holding to know which keys are in it.
///
/// Callers gate on `report.truncated`; this returns the disclosure for a run already known to be
/// truncated.
fn truncation_properties(report: &LintReport) -> serde_json::Map<String, serde_json::Value> {
    let mut props = serde_json::Map::new();
    props.insert("truncated".into(), json!(true));
    props.insert("truncatedCount".into(), json!(report.truncated_count));
    props.insert("appliedCap".into(), json!(report.applied_cap));
    props
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
