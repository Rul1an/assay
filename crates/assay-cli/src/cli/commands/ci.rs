use super::super::args::CiArgs;
use super::pipeline::{
    build_summary_from_artifacts, execute_pipeline, maybe_export_baseline, print_pipeline_summary,
    write_error_artifacts, PipelineError, PipelineInput,
};
use super::run_output::write_extended_run_json;
use std::path::PathBuf;

pub(crate) async fn run(args: CiArgs, legacy_mode: bool) -> anyhow::Result<i32> {
    let version = args.exit_codes;
    let run_json_path = PathBuf::from("run.json");

    let input = PipelineInput::from_ci(&args);
    let execution = execute_pipeline(&input, legacy_mode).await;
    let execution = match execution {
        Ok(ok) => ok,
        Err(PipelineError::Classified { reason, message }) => {
            return write_error_artifacts(
                reason,
                message,
                version,
                !args.no_verify,
                &run_json_path,
            );
        }
        Err(PipelineError::Fatal(err)) => return Err(err),
    };

    if execution.cfg.version > 0 {
        eprintln!("Loaded config version: {}", execution.cfg.version);
    }

    let cfg = execution.cfg;
    let artifacts = execution.artifacts;
    // Determine outcome first (safety against report write failures)
    let mut outcome = execution.outcome;

    // Write output formats (best effort); SARIF outcome needed for run.json/summary sarif.omitted
    if let Err(e) = (|| -> anyhow::Result<()> {
        if let Some(parent) = args.junit.parent() {
            std::fs::create_dir_all(parent)?;
        }
        assay_core::report::junit::write_junit(&cfg.suite, &artifacts.results, &args.junit)?;
        Ok(())
    })() {
        let msg = format!("Failed to write JUnit report: {}", e);
        eprintln!("WARNING: {}", msg);
        outcome.warnings.push(msg);
    }

    let sarif_outcome = (|| -> anyhow::Result<assay_core::report::sarif::SarifWriteOutcome> {
        if let Some(parent) = args.sarif.parent() {
            std::fs::create_dir_all(parent)?;
        }
        assay_core::report::sarif::write_sarif("assay", &artifacts.results, &args.sarif)
    })();
    let sarif_omitted = match &sarif_outcome {
        Ok(o) => o.omitted_count,
        Err(e) => {
            outcome
                .warnings
                .push(format!("Failed to write SARIF report: {}", e));
            0
        }
    };

    write_extended_run_json(&artifacts, &outcome, &run_json_path, Some(sarif_omitted))?;

    let summary_path = run_json_path
        .parent()
        .map(|p| p.join("summary.json"))
        .unwrap_or_else(|| PathBuf::from("summary.json"));
    let mut summary = build_summary_from_artifacts(&outcome, !args.no_verify, &artifacts);
    summary = summary.with_sarif_omitted(sarif_omitted);
    assay_core::report::summary::write_summary(&summary, &summary_path)?;

    print_pipeline_summary(&artifacts, args.explain_skip, &summary);

    let otel_cfg = assay_core::otel::OTelConfig {
        jsonl_path: args.otel_jsonl.clone(),
        redact_prompts: args.redact_prompts,
    };
    let _ = assay_core::otel::export_jsonl(&otel_cfg, &cfg.suite, &artifacts.results);

    maybe_export_baseline(&args.export_baseline, &args.config, &cfg, &artifacts);

    // Write PR comment markdown if requested
    if let Some(comment_path) = &args.pr_comment {
        let md = format_pr_comment(&outcome, &summary, &cfg.suite);
        if let Some(parent) = comment_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(comment_path, &md)?;
        eprintln!("Wrote PR comment to {}", comment_path.display());
    }

    Ok(outcome.exit_code)
}

/// Format a PR comment body as markdown for GitHub Actions.
fn format_pr_comment(
    outcome: &crate::exit_codes::RunOutcome,
    summary: &assay_core::report::summary::Summary,
    suite: &str,
) -> String {
    let mut md = String::new();
    let suite_display = escape_markdown_table_cell(suite);

    // Marker for find/update pattern
    md.push_str("<!-- assay-governance-report -->\n");
    md.push_str("## Assay Governance Report\n\n");

    // Status line
    let status = if outcome.exit_code == 0 {
        "**Status:** :white_check_mark: Pass"
    } else {
        "**Status:** :x: Fail"
    };
    md.push_str(status);
    md.push('\n');
    md.push('\n');

    // Results table
    md.push_str("| Metric | Value |\n");
    md.push_str("|--------|-------|\n");
    md.push_str(&format!("| Suite | {} |\n", suite_display));

    if let Some(results) = &summary.results {
        let total = results.total;
        md.push_str(&format!(
            "| Tests | {}/{} passed |\n",
            results.passed, total
        ));
        if results.failed > 0 {
            md.push_str(&format!("| Failed | {} |\n", results.failed));
        }
    }

    md.push_str(&format!("| Exit | {} |\n", outcome.exit_code));

    if !outcome.reason_code.is_empty() {
        md.push_str(&format!("| Reason | `{}` |\n", outcome.reason_code));
    }

    // Warnings
    if !outcome.warnings.is_empty() {
        md.push('\n');
        md.push_str("<details>\n<summary>Warnings</summary>\n\n");
        for w in &outcome.warnings {
            md.push_str(&format!("- {}\n", escape_markdown_text(w)));
        }
        md.push_str("\n</details>\n");
    }

    // Conversion hint
    md.push('\n');
    md.push_str(
        "> For EU AI Act compliance scanning: `assay evidence lint --pack eu-ai-act-baseline`\n",
    );

    // Footer
    md.push('\n');
    md.push_str("---\n");
    md.push_str(&format!(
        "Generated by Assay v{}\n",
        env!("CARGO_PKG_VERSION")
    ));

    md
}

fn escape_markdown_table_cell(input: &str) -> String {
    escape_markdown_text(input).replace('|', "\\|")
}

fn escape_markdown_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '\r' | '\n' => out.push(' '),
            '\\' | '`' | '*' | '_' | '[' | ']' | '#' | '<' | '>' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}
