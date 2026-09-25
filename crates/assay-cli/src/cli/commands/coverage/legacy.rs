use super::io::{print_markdown_report, print_text_report};
use super::LegacyOutputFormat;
use crate::cli::args::CoverageArgs;
use crate::cli_failure::CliFailure;
use anyhow::{Context, Result};

pub(super) async fn cmd_coverage_legacy(
    args: CoverageArgs,
    output: LegacyOutputFormat,
) -> Result<i32> {
    let trace_file = match args.trace_file.as_ref() {
        Some(path) => path,
        None => {
            return Err(CliFailure::coverage_invalid_args(
                "--trace-file/--traces is required when --input is not used",
            )
            .into());
        }
    };

    // 1. Determine Policy & Context
    let (policy_path, suite_name, config_fingerprint) = if let Some(p) = args.policy {
        // Explicit Policy Mode
        let suite = p
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("manual_policy")
            .to_string();

        // Fingerprint the policy file itself as the "config"
        let fp = assay_core::baseline::compute_config_fingerprint(&p);

        (p, suite, fp)
    } else {
        // Fallback: Try to infer from config (Legacy)
        let cfg = assay_core::config::load_config(&args.config, false, false)
            .context("failed to load config (and no --policy provided)")?;

        let mut policy_paths = std::collections::HashSet::new();
        for test in &cfg.tests {
            if let Some(path) = test.expected.get_policy_path() {
                policy_paths.insert(path.to_string());
            }
        }

        if policy_paths.is_empty() {
            anyhow::bail!("No policy provided via --policy, and none referenced in config.");
        }

        if policy_paths.len() > 1 {
            eprintln!(
                "warning: multiple policies found in config: {:?}. Using the first one.",
                policy_paths
            );
        }

        // Resolve relative to config file
        let rel = policy_paths.iter().next().unwrap();
        let config_dir = args.config.parent().unwrap_or(std::path::Path::new("."));
        let policy_path = config_dir.join(rel);

        let fp = assay_core::baseline::compute_config_fingerprint(&args.config);

        (policy_path, cfg.suite, fp)
    };

    // 2. Load Policy
    let policy_content = tokio::fs::read_to_string(&policy_path)
        .await
        .with_context(|| format!("failed to read policy file: {}", policy_path.display()))?;

    // Compliance Check runs on V2 engine (McpPolicy)
    let mut policy_v2: assay_core::mcp::policy::McpPolicy =
        serde_yaml::from_str(&policy_content).context("failed to parse policy yaml")?;

    // Normalize shapes (e.g. root allow/deny -> tools.allow/deny)
    policy_v2.normalize_legacy_shapes();

    // Auto-migrate v1 constraints if present (critical for hybrid policies)
    if !policy_v2.constraints.is_empty() {
        policy_v2.migrate_constraints_to_schemas();
    }

    // Coverage Analysis runs on Legacy engine (model::Policy)
    // Try to parse strictly as Legacy Policy. If fail, synthesize from V2.
    let policy: assay_core::model::Policy = match serde_yaml::from_str(&policy_content) {
        Ok(p) => p,
        Err(_) => {
            // Synthesize legacy policy for CoverageAnalyzer
            assay_core::model::Policy {
                version: policy_v2.version.clone(),
                name: policy_v2.name.clone(),
                metadata: None,
                tools: assay_core::model::ToolsPolicy {
                    allow: policy_v2.tools.allow.clone(),
                    deny: policy_v2.tools.deny.clone(),
                    require_args: None,
                    arg_constraints: None,
                },
                sequences: vec![],
                aliases: std::collections::HashMap::new(),
                on_error: assay_core::on_error::ErrorPolicy::default(),
            }
        }
    };

    // 3. Load Traces
    let file_content: String = tokio::fs::read_to_string(trace_file)
        .await
        .context("failed to read trace file")?;

    let mut trace_records = Vec::new();

    // Prepare for validation; a policy whose schemas cannot compile has no coverage story.
    policy_v2
        .try_compile_all_schemas()
        .map_err(|error| anyhow::anyhow!("policy schemas failed to compile: {error}"))?;
    let mut state = assay_core::mcp::policy::PolicyState::default();

    let mut violations = Vec::new();
    let mut warnings = Vec::new();

    // Parse all lines as Value
    let mut events_by_id: std::collections::HashMap<String, Vec<serde_json::Value>> =
        std::collections::HashMap::new();

    for line in file_content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let v: serde_json::Value = serde_json::from_str(line).context("invalid jsonl")?;

        let id_val = v
            .get("test_id")
            .or_else(|| v.get("episode_id"))
            .or_else(|| v.get("run_id"))
            .or_else(|| v.get("id"));

        let id = if let Some(id_s) = id_val.and_then(|s| s.as_str()) {
            id_s.to_string()
        } else {
            "unknown".to_string()
        };

        events_by_id.entry(id).or_default().push(v);
    }

    for (id, events) in events_by_id {
        let mut tools_called = Vec::new();
        let mut calls: Vec<assay_core::sequence_eval::SequenceCall> = Vec::new();

        for event in events {
            if let Some(typ) = event.get("type").and_then(|s| s.as_str()) {
                if typ == "call_tool" {
                    let tool_opt = event
                        .get("tool_name")
                        .or_else(|| event.get("tool"))
                        .and_then(|s| s.as_str());

                    if let Some(tool) = tool_opt {
                        let tool_name = tool.to_string();
                        tools_called.push(tool_name.clone());

                        // Validate compliance (Unified V2)
                        let args_default = serde_json::json!({});
                        let args = event
                            .get("arguments")
                            .or_else(|| event.get("input")) // fallback for some formats
                            .unwrap_or(&args_default);

                        calls.push(assay_core::sequence_eval::SequenceCall {
                            name: tool_name.clone(),
                            args: args.clone(),
                        });

                        let decision = policy_v2.evaluate(&tool_name, args, &mut state, None);

                        match decision {
                            assay_core::mcp::policy::PolicyDecision::Allow => {}
                            assay_core::mcp::policy::PolicyDecision::AllowWithWarning {
                                code,
                                reason,
                                ..
                            } => {
                                warnings.push(assay_core::coverage::PolicyWarning {
                                    trace_id: id.clone(),
                                    tool: tool_name.clone(),
                                    warning_code: code,
                                    reason,
                                });
                            }
                            assay_core::mcp::policy::PolicyDecision::Deny {
                                code, reason, ..
                            } => {
                                violations.push(assay_core::coverage::PolicyViolation {
                                    trace_id: id.clone(),
                                    tool: tool_name.clone(),
                                    error_code: code,
                                    reason,
                                });
                            }
                        }
                    }
                }
            }
            if let Some(tools) = event.get("tools").and_then(|v| v.as_array()) {
                for t in tools {
                    if let Some(s) = t.as_str() {
                        tools_called.push(s.to_string());
                        calls.push(assay_core::sequence_eval::SequenceCall::named(s));
                    }
                }
            }
        }

        if !tools_called.is_empty() {
            let rules_triggered = assay_core::coverage::triggered_rules(&policy, &calls);
            trace_records.push(assay_core::coverage::TraceRecord {
                trace_id: id,
                tools_called,
                rules_triggered,
            });
        }
    }

    if trace_records.is_empty() {
        eprintln!("warning: no tool calls found in trace file");
    }

    // 4. Analyze
    let analyzer = assay_core::coverage::CoverageAnalyzer::from_policy(&policy);
    let mut report = analyzer.analyze(&trace_records, args.min_coverage);

    // Attach discovered violations/warnings
    report.policy_violations = violations;
    report.policy_warnings = warnings;

    // 5. Output
    match output {
        LegacyOutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        LegacyOutputFormat::Markdown => {
            print_markdown_report(&report);
        }
        LegacyOutputFormat::Text => {
            print_text_report(&report);
        }
    }

    let mut clean_pass = true;

    // 6. Export Baseline (if requested)
    if let Some(export_path) = args.export_baseline {
        // Capture git info if possible
        let git_info = crate::cli::commands::baseline::capture_git_info(); // Reuse logic from baseline.rs

        let baseline = assay_core::baseline::Baseline::from_coverage_report(
            &report,
            suite_name.clone(),
            config_fingerprint.clone(),
            git_info,
        );

        baseline
            .save(&export_path)
            .context("failed to save baseline")?;
        eprintln!("Exported baseline to {}", export_path.display());
    }

    // 7. Check Baseline Regression (if requested)
    if let Some(baseline_path) = args.baseline {
        let baseline = assay_core::baseline::Baseline::load(&baseline_path)
            .context("failed to load baseline for comparison")?;

        // Construct candidate strictly for diffing logic (reuse from_coverage_report)
        let candidate = assay_core::baseline::Baseline::from_coverage_report(
            &report,
            suite_name.clone(),
            config_fingerprint.clone(),
            None, // Git info optional for candidte diff? No, let's capture it.
        );

        let diff = baseline.diff(&candidate);

        // A dimension that turned not applicable at an equal score never
        // enters `diff.regressions` (Baseline::diff compares scores only), so
        // without this it goes silently clean while the overall mean rises.
        // An applicable baseline entry (meta absent or exercised) whose
        // candidate entry is exactly not_applicable is a failing compare at
        // any score, including 0.0 -> 0.0. Collect it before choosing the
        // banner: the clean line prints only when there is no regression
        // AND no such transition, never alongside the regression block.
        let mut equal_score_na = Vec::new();
        for entry in &baseline.entries {
            if candidate.is_not_applicable(&entry.test_id, &entry.metric)
                && !baseline.is_not_applicable(&entry.test_id, &entry.metric)
                && !diff
                    .regressions
                    .iter()
                    .any(|r| r.test_id == entry.test_id && r.metric == entry.metric)
            {
                equal_score_na.push(entry.clone());
            }
        }

        if !diff.regressions.is_empty() || !equal_score_na.is_empty() {
            eprintln!("\n❌ REGRESSION DETECTED against baseline:");
            for r in &diff.regressions {
                eprintln!(
                    "  - {} metric '{}': {:.2}% -> {:.2}% (delta: {:.2}%)",
                    r.test_id, r.metric, r.baseline_score, r.candidate_score, r.delta
                );
                // A candidate entry marked not_applicable stays a failing
                // compare: name the transition and point at re-export.
                if candidate.is_not_applicable(&r.test_id, &r.metric) {
                    if let Some(note) = not_applicable_regression_note(&r.metric, r.baseline_score)
                    {
                        eprintln!("    {note}");
                    }
                }
            }
            for entry in &equal_score_na {
                let candidate_score = candidate
                    .get_score(&entry.test_id, &entry.metric)
                    .unwrap_or(0.0);
                eprintln!(
                    "  - {} metric '{}': {:.2}% -> {:.2}% (delta: {:.2}%)",
                    entry.test_id,
                    entry.metric,
                    entry.score,
                    candidate_score,
                    candidate_score - entry.score
                );
                if let Some(note) = not_applicable_regression_note(&entry.metric, entry.score) {
                    eprintln!("    {note}");
                }
            }
            clean_pass = false;
        } else {
            eprintln!("\n✅ No regression against baseline.");
        }

        for i in &diff.improvements {
            // A baseline entry marked not_applicable that is applicable again
            // is a newly measured dimension, not progress: announce it as such.
            if baseline.is_not_applicable(&i.test_id, &i.metric)
                && !candidate.is_not_applicable(&i.test_id, &i.metric)
            {
                eprintln!("    {}", newly_applicable_note(&i.metric, i.baseline_score));
            }
        }
    }

    // 8. Exit checks

    // Check 1: Policy Violations
    if !report.policy_violations.is_empty() {
        eprintln!("\n🚨 ERROR: Policy Violations Detected in Traces!");
        for v in &report.policy_violations {
            eprintln!(
                "  - [{}][{}] {} ({})",
                v.trace_id, v.tool, v.reason, v.error_code
            );
        }
        clean_pass = false;
    }

    if !report.policy_warnings.is_empty() {
        eprintln!("\n⚠️ Policy Warnings:");
        for w in &report.policy_warnings {
            eprintln!(
                "  - [{}][{}] {} ({})",
                w.trace_id, w.tool, w.reason, w.warning_code
            );
        }
    }

    // Check 2: High Risk Gaps
    if !report.high_risk_gaps.is_empty() {
        eprintln!("\n🚨 ERROR: High Risk Gaps Detected!");
        eprintln!("The following DENY-listed tools were not tested:");
        for gap in &report.high_risk_gaps {
            eprintln!("  - {}", gap.tool);
        }
        clean_pass = false;
    }

    // Check 2: Min Coverage. A report with no applicable dimension refuses its
    // threshold with the stated reason; the exit stays TEST_FAILED (1) through
    // this existing branch, not a new exit code.
    if !report.meets_threshold {
        if let Some(reason) = report.not_applicable_reason() {
            eprintln!("\n❌ {reason}");
        } else {
            eprintln!(
                "\n❌ Minimum coverage not met ({:.1}% < {:.1}%)",
                report.overall_coverage_pct, report.threshold
            );
        }
        clean_pass = false;
    }

    if clean_pass {
        Ok(crate::exit_codes::OK)
    } else {
        Ok(crate::exit_codes::TEST_FAILED)
    }
}

/// Sentence for a regression whose candidate entry is not applicable: the
/// dimension disappeared from the policy, so the drop to 0 is named and the
/// reader is pointed at re-export. Stays a failing compare; `None` for
/// metrics outside the coverage vocabulary.
fn not_applicable_regression_note(metric: &str, baseline_score: f64) -> Option<String> {
    let declared = match metric {
        "tool" => "0 tools declared",
        "rule" => "0 rules declared",
        "overall" => "0 tools and 0 rules declared",
        _ => return None,
    };
    Some(format!(
        "'{metric}' is not applicable in the current policy ({declared}); \
         baseline recorded {baseline_score:.1}. \
         Re-export with --export-baseline if intentional."
    ))
}

/// Notice for an improvement whose baseline entry was not_applicable: the
/// dimension is measured again, which is not the same as improved coverage.
fn newly_applicable_note(metric: &str, baseline_score: f64) -> String {
    format!(
        "'{metric}' is newly applicable in the current policy; \
         baseline recorded {baseline_score:.1} as not applicable. \
         This is a newly measured dimension, not improved coverage."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_applicable_note_names_rule_transition_with_re_export_hint() {
        assert_eq!(
            not_applicable_regression_note("rule", 100.0).as_deref(),
            Some(
                "'rule' is not applicable in the current policy (0 rules declared); \
                 baseline recorded 100.0. \
                 Re-export with --export-baseline if intentional."
            )
        );
    }

    #[test]
    fn not_applicable_note_names_tool_transition() {
        assert_eq!(
            not_applicable_regression_note("tool", 60.0).as_deref(),
            Some(
                "'tool' is not applicable in the current policy (0 tools declared); \
                 baseline recorded 60.0. \
                 Re-export with --export-baseline if intentional."
            )
        );
    }

    #[test]
    fn not_applicable_note_refuses_unknown_metrics() {
        assert_eq!(not_applicable_regression_note("semantic", 60.0), None);
    }

    #[test]
    fn newly_applicable_note_announces_rule_remeasurement() {
        assert_eq!(
            newly_applicable_note("rule", 0.0),
            "'rule' is newly applicable in the current policy; \
             baseline recorded 0.0 as not applicable. \
             This is a newly measured dimension, not improved coverage."
        );
    }
}
