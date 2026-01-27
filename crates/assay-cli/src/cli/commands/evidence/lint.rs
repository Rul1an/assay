use anyhow::{Context, Result};
use assay_evidence::lint::engine::lint_bundle;
use assay_evidence::lint::sarif::to_sarif;
use assay_evidence::lint::Severity;
use assay_evidence::VerifyLimits;
use clap::Args;
use std::fs::File;

#[derive(Debug, Args, Clone)]
pub struct LintArgs {
    /// Bundle to lint
    #[arg(value_name = "BUNDLE")]
    pub bundle: std::path::PathBuf,

    /// Output format: json, sarif, or text
    #[arg(long, default_value = "text")]
    pub format: String,

    /// Fail (exit 1) if findings at or above this severity exist
    #[arg(long, default_value = "error")]
    pub fail_on: String,
}

pub fn cmd_lint(args: LintArgs) -> Result<i32> {
    let f = File::open(&args.bundle)
        .with_context(|| format!("failed to open bundle {}", args.bundle.display()))?;

    let limits = VerifyLimits::default();

    let report = match lint_bundle(f, limits) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Verification failed: {}", e);
            return Ok(2);
        }
    };

    match args.format.as_str() {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        "sarif" => {
            let sarif = to_sarif(&report);
            println!("{}", serde_json::to_string_pretty(&sarif)?);
        }
        _ => {
            eprintln!("Assay Evidence Lint");
            eprintln!("===================");
            eprintln!(
                "Bundle: {} (events: {}, verified: {})",
                report.bundle_meta.bundle_id, report.bundle_meta.event_count, report.verified
            );
            eprintln!();

            if report.findings.is_empty() {
                eprintln!("No findings.");
            } else {
                for finding in &report.findings {
                    let loc_str = match &finding.location {
                        Some(loc) => format!("seq:{} line:{}", loc.seq, loc.line),
                        None => "global".into(),
                    };
                    eprintln!(
                        "[{}] {} ({}) {}",
                        finding.severity, finding.rule_id, loc_str, finding.message
                    );
                }
                eprintln!();
                eprintln!(
                    "Summary: {} total ({} errors, {} warnings, {} info)",
                    report.summary.total,
                    report.summary.errors,
                    report.summary.warnings,
                    report.summary.infos
                );
            }
        }
    }

    // Exit codes: 0 = no findings at/above threshold, 1 = findings found, 2 = verification failure
    let threshold = match args.fail_on.as_str() {
        "error" => Severity::Error,
        "warn" | "warning" => Severity::Warn,
        "info" => Severity::Info,
        _ => Severity::Error,
    };

    if report.has_findings_at_or_above(&threshold) {
        Ok(1)
    } else {
        Ok(0)
    }
}
