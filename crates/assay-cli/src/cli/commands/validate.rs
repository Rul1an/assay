use assay_core::config::{load_config, path_resolver::PathResolver};
use assay_core::errors::diagnostic::Diagnostic;
use assay_core::validate::{validate, ValidateOptions, ValidateReport};
use serde_json::json;

use crate::cli::args::ValidateArgs;

pub async fn run(args: ValidateArgs, legacy_mode: bool) -> anyhow::Result<i32> {
    // 1. Load Config
    let cfg = match load_config(&args.config, legacy_mode) {
        Ok(c) => c,
        Err(e) => {
            // If config fails to load, that's an E_CFG_PARSE or similar.
            // We construct a diagnostic manually here because we can't run validate() without config.
            let diag = Diagnostic::new(
                assay_core::errors::diagnostic::codes::E_CFG_PARSE,
                format!("Failed to load config: {}", e),
            )
            .with_source("config")
            .with_context(json!({ "file": args.config }));

            print_report(
                &ValidateReport {
                    diagnostics: vec![diag],
                },
                &args.format,
            );
            return Ok(2);
        }
    };

    let resolver = PathResolver::new(&args.config);

    // 2. Prepare Options
    // Note: CLI args override config if user passes them explicitly?
    // In validate command, we usually validte what IS passed.
    let opts = ValidateOptions {
        trace_file: args.trace_file,
        baseline_file: args.baseline,
        replay_strict: args.replay_strict,
    };

    // 3. Run Validation
    let report = validate(&cfg, &opts, &resolver).await?;

    // 4. Print Report
    print_report(&report, &args.format);

    // 5. Determine Exit Code
    // Any error severity -> 2. Warnings only -> 0.
    if report.diagnostics.iter().any(|d| d.severity == "error") {
        Ok(2)
    } else {
        Ok(0)
    }
}

fn print_report(report: &ValidateReport, format: &str) {
    if format == "json" {
        let errors: Vec<&Diagnostic> = report
            .diagnostics
            .iter()
            .filter(|d| d.severity == "error")
            .collect();
        let warnings: Vec<&Diagnostic> = report
            .diagnostics
            .iter()
            .filter(|d| d.severity == "warn")
            .collect();
        let ok = errors.is_empty();

        let output = json!({
            "schema_version": 1,
            "ok": ok,
            "errors": errors,
            "warnings": warnings,
            "summary": {
                // We could populate this from the report struct if we added it
                "diagnostic_count": report.diagnostics.len()
            }
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        // Text format
        let errors_count = report
            .diagnostics
            .iter()
            .filter(|d| d.severity == "error")
            .count();
        let warnings_count = report
            .diagnostics
            .iter()
            .filter(|d| d.severity == "warn")
            .count();

        if errors_count > 0 {
            eprintln!(
                "✖ Validation failed ({} error{}, {} warning{})",
                errors_count,
                if errors_count != 1 { "s" } else { "" },
                warnings_count,
                if warnings_count != 1 { "s" } else { "" }
            );
        } else if warnings_count > 0 {
            eprintln!(
                "⚠️  Validation passed with warnings ({} warning{})",
                warnings_count,
                if warnings_count != 1 { "s" } else { "" }
            );
        } else {
            eprintln!("✔ Validation OK");
        }
        eprintln!();

        for d in &report.diagnostics {
            eprintln!("{}", d.format_terminal());
        }
    }
}
