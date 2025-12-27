use crate::cli::args::BaselineReportArgs;
use assay_core::baseline::report::report_from_db;
use assay_core::storage::Store;
use std::fs;

pub fn cmd_baseline_report(args: BaselineReportArgs) -> anyhow::Result<()> {
    let store = Store::open(&args.db)?;
    store.init_schema()?;
    let report = report_from_db(&store, &args.suite, args.last)?;

    if args.format == "json" && args.out.extension().is_none_or(|ext| ext == "json") {
        let json = serde_json::to_string_pretty(&report)?;
        std::fs::write(&args.out, json)?;
        eprintln!("wrote {}", args.out.display());
    } else if args.format == "md" || args.out.extension().is_some_and(|ext| ext == "md") {
        let md = generate_markdown(&report);
        fs::write(&args.out, md)?;
        eprintln!("wrote {}", args.out.display());
    } else {
        // Default to JSON
        let json = serde_json::to_string_pretty(&report)?;
        std::fs::write(&args.out, json)?;
        eprintln!("wrote {}", args.out.display());
    }

    Ok(())
}

fn generate_markdown(report: &assay_core::baseline::report::HygieneReport) -> String {
    let mut md = String::new();

    md.push_str(&format!("# Baseline Hygiene Report: {}\n\n", report.suite));
    md.push_str(&format!(
        "**Source**: `{}` | **Generated**: {} | **Window**: Last {} runs\n\n",
        report.source, report.generated_at, report.window.last_runs
    ));

    if !report.notes.is_empty() {
        md.push_str("### ⚠️ Notes\n");
        for note in &report.notes {
            md.push_str(&format!("- {}\n", note));
        }
        md.push('\n');
    }

    md.push_str("### Test Stability & Performance\n\n");
    md.push_str("| Test ID | N | Pass | Fail | Flaky | Skip | P90 Score (SemSim) | Top Issues |\n");
    md.push_str("|---|---|---|---|---|---|---|---|\n");

    for t in &report.tests {
        let semsim = t
            .scores
            .get("semantic_similarity_to")
            .map(|s| format!("{:.2}", s.p90))
            .unwrap_or_else(|| "-".to_string());

        let issues: Vec<String> = t
            .top_reasons
            .iter()
            .map(|r| format!("{}: {} ({})", r.kind, r.value, r.count))
            .collect();
        let issues_str = if issues.is_empty() {
            "-".to_string()
        } else {
            issues.join("<br>")
        };

        md.push_str(&format!(
            "| `{}` | {} | {:.0}% | {:.0}% | {:.0}% | {:.0}% | {} | {} |\n",
            t.test_id,
            t.n,
            t.rates.pass * 100.0,
            t.rates.fail * 100.0,
            t.rates.flaky * 100.0,
            t.rates.skipped * 100.0,
            semsim,
            issues_str
        ));
    }

    md
}
