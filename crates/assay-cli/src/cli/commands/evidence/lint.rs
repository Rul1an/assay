use anyhow::{Context, Result};
use assay_evidence::lint::engine::{lint_bundle_with_options, LintOptions};
use assay_evidence::lint::packs::load_packs;
use assay_evidence::lint::sarif::{to_sarif_with_options, SarifOptions};
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

    /// Comma-separated pack references (built-in name or file path)
    #[arg(long, value_delimiter = ',')]
    pub pack: Option<Vec<String>>,

    /// Maximum results in output (for GitHub SARIF limits)
    #[arg(long, default_value = "500")]
    pub max_results: usize,
}

pub fn cmd_lint(args: LintArgs) -> Result<i32> {
    let f = File::open(&args.bundle)
        .with_context(|| format!("failed to open bundle {}", args.bundle.display()))?;

    let limits = VerifyLimits::default();

    // Load packs if specified
    let packs = if let Some(pack_refs) = &args.pack {
        match load_packs(pack_refs) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Pack loading failed: {}", e);
                return Ok(3); // Exit code 3 = pack error
            }
        }
    } else {
        vec![]
    };

    // Build lint options
    let options = LintOptions {
        packs,
        max_results: Some(args.max_results),
        bundle_path: Some(args.bundle.display().to_string()),
    };

    let result = match lint_bundle_with_options(f, limits, options) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Verification failed: {}", e);
            return Ok(2);
        }
    };

    let report = &result.report;
    let pack_meta = &result.pack_meta;

    match args.format.as_str() {
        "json" => {
            // Add disclaimer to JSON output for compliance packs
            let mut json_report = serde_json::to_value(report)?;
            if let Some(meta) = pack_meta {
                if let Some(disclaimer) = &meta.disclaimer {
                    json_report
                        .as_object_mut()
                        .unwrap()
                        .insert("disclaimer".into(), serde_json::json!(disclaimer));
                }
                if meta.truncated {
                    json_report
                        .as_object_mut()
                        .unwrap()
                        .insert("truncated".into(), serde_json::json!(true));
                    json_report.as_object_mut().unwrap().insert(
                        "truncated_count".into(),
                        serde_json::json!(meta.truncated_count),
                    );
                }
            }
            println!("{}", serde_json::to_string_pretty(&json_report)?);
        }
        "sarif" => {
            #[allow(deprecated)]
            let sarif_options = SarifOptions {
                pack_meta: pack_meta.clone(),
                bundle_path: Some(args.bundle.display().to_string()),
                working_directory: None, // Deprecated: no longer included in output
            };
            let sarif = to_sarif_with_options(report, sarif_options);
            println!("{}", serde_json::to_string_pretty(&sarif)?);
        }
        _ => {
            eprintln!("Assay Evidence Lint");
            eprintln!("===================");
            eprintln!(
                "Bundle: {} (events: {}, verified: {})",
                report.bundle_meta.bundle_id, report.bundle_meta.event_count, report.verified
            );

            // Print pack info
            if let Some(meta) = pack_meta {
                eprintln!(
                    "Packs: {}",
                    meta.packs
                        .iter()
                        .map(|p| format!("{}@{}", p.name, p.version))
                        .collect::<Vec<_>>()
                        .join(", ")
                );

                // Print disclaimer for compliance packs
                if let Some(disclaimer) = &meta.disclaimer {
                    eprintln!();
                    eprintln!("⚠️  COMPLIANCE DISCLAIMER");
                    eprintln!("{}", disclaimer);
                }

                if meta.truncated {
                    eprintln!();
                    eprintln!(
                        "⚠️  Results truncated: {} findings omitted (--max-results {})",
                        meta.truncated_count, args.max_results
                    );
                }
            }
            eprintln!();

            if report.findings.is_empty() {
                eprintln!("No findings.");
            } else {
                for finding in &report.findings {
                    let loc_str = match &finding.location {
                        Some(loc) => format!("seq:{} line:{}", loc.seq, loc.line),
                        None => "global".into(),
                    };

                    // Extract article_ref from tags if present
                    let article_ref = finding
                        .tags
                        .iter()
                        .find(|t| t.starts_with("article_ref:"))
                        .map(|t| t.strip_prefix("article_ref:").unwrap_or(""));

                    if let Some(ref_) = article_ref {
                        eprintln!(
                            "[{}] {} ({}) {} [Article {}]",
                            finding.severity, finding.rule_id, loc_str, finding.message, ref_
                        );
                    } else {
                        eprintln!(
                            "[{}] {} ({}) {}",
                            finding.severity, finding.rule_id, loc_str, finding.message
                        );
                    }
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

    // Exit codes: 0 = no findings at/above threshold, 1 = findings found, 2 = verification failure, 3 = pack error
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
