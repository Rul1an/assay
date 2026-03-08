use super::format_md;
use super::CoverageOutputFormat;
use crate::exit_codes;
use anyhow::Result;
use serde_json::Value;
use std::path::Path;

pub(super) async fn write_generated_coverage_payload(
    out: &Path,
    report_value: &Value,
    format: CoverageOutputFormat,
    routes_top: usize,
) -> Result<i32> {
    use crate::exit_codes::EXIT_INFRA_ERROR;

    let Some(parent) = out.parent() else {
        eprintln!("Infra error: invalid output path {}", out.display());
        return Ok(EXIT_INFRA_ERROR);
    };

    if !parent.as_os_str().is_empty() {
        if let Err(e) = tokio::fs::create_dir_all(parent).await {
            eprintln!("Infra error: failed to prepare {}: {e}", parent.display());
            return Ok(EXIT_INFRA_ERROR);
        }
    }

    let payload = match format {
        CoverageOutputFormat::Json => serde_json::to_vec_pretty(&report_value)
            .expect("coverage report serialization should be infallible"),
        CoverageOutputFormat::Markdown => {
            match format_md::render_coverage_markdown(report_value, routes_top) {
                Ok(markdown) => markdown.into_bytes(),
                Err(e) => {
                    eprintln!("Measurement error: failed to render markdown output: {e}");
                    return Ok(exit_codes::EXIT_CONFIG_ERROR);
                }
            }
        }
    };

    if let Err(e) = tokio::fs::write(out, payload).await {
        eprintln!(
            "Infra error: failed to write coverage report to {}: {e}",
            out.display()
        );
        return Ok(EXIT_INFRA_ERROR);
    }

    match format {
        CoverageOutputFormat::Json => eprintln!("Wrote coverage_report_v1 to {}", out.display()),
        CoverageOutputFormat::Markdown => {
            eprintln!("Wrote coverage_report_v1 markdown to {}", out.display())
        }
    }

    Ok(exit_codes::EXIT_SUCCESS)
}

pub(super) fn print_text_report(report: &assay_core::coverage::CoverageReport) {
    println!("Coverage Report");
    println!("===============");
    println!(
        "Overall: {:.1}% (Threshold: {:.1}%)",
        report.overall_coverage_pct, report.threshold
    );
    println!();
    println!("Tool Coverage: {:.1}%", report.tool_coverage.coverage_pct);
    println!(
        "  Seen: {}/{}",
        report.tool_coverage.tools_seen_in_traces, report.tool_coverage.total_tools_in_policy
    );
    if !report.tool_coverage.unseen_tools.is_empty() {
        println!("  Unseen Tools:");
        for t in &report.tool_coverage.unseen_tools {
            println!("    - {}", t);
        }
    }
    println!();
    println!("Rule Coverage: {:.1}%", report.rule_coverage.coverage_pct);

    if !report.high_risk_gaps.is_empty() {
        println!();
        println!("HIGH RISK GAPS DETECTED:");
        for gap in &report.high_risk_gaps {
            println!("  [!] {}: {}", gap.tool, gap.reason);
        }
    }

    if !report.policy_violations.is_empty() {
        println!();
        println!("POLICY VIOLATIONS:");
        for v in &report.policy_violations {
            println!("  [x] {} {}: {}", v.trace_id, v.tool, v.reason);
        }
    }

    if !report.policy_warnings.is_empty() {
        println!();
        println!("POLICY WARNINGS:");
        for w in &report.policy_warnings {
            println!("  [!] {} {}: {}", w.trace_id, w.tool, w.reason);
        }
    }
}

pub(super) fn print_markdown_report(report: &assay_core::coverage::CoverageReport) {
    println!("# Coverage Report");
    println!(
        "**Overall**: {:.1}% (Threshold: {:.1}%)",
        report.overall_coverage_pct, report.threshold
    );

    println!(
        "## Tool Coverage: {:.1}%",
        report.tool_coverage.coverage_pct
    );
    println!(
        "- Seen: {}/{}",
        report.tool_coverage.tools_seen_in_traces, report.tool_coverage.total_tools_in_policy
    );

    if !report.tool_coverage.unseen_tools.is_empty() {
        println!("### Unseen Tools");
        for t in &report.tool_coverage.unseen_tools {
            println!("- {}", t);
        }
    }

    if !report.high_risk_gaps.is_empty() {
        println!("## 🚨 High Risk Gaps in Coverage");
        for gap in &report.high_risk_gaps {
            println!("- **{}**: {}", gap.tool, gap.reason);
        }
    }

    if !report.policy_violations.is_empty() {
        println!("## ❌ Policy Violations");
        for v in &report.policy_violations {
            println!(
                "- {}: **{}** - {} (`{}`)",
                v.trace_id, v.tool, v.reason, v.error_code
            );
        }
    }

    if !report.policy_warnings.is_empty() {
        println!("## ⚠️ Policy Warnings");
        for w in &report.policy_warnings {
            println!(
                "- {}: **{}** - {} (`{}`)",
                w.trace_id, w.tool, w.reason, w.warning_code
            );
        }
    }
}
