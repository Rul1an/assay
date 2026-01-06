use assay_core::config::{load_config, path_resolver::PathResolver};
use assay_core::errors::diagnostic::Diagnostic;
use assay_core::validate::{validate, ValidateOptions, ValidateReport};
use serde_json::json;

use crate::cli::args::ValidateArgs;

pub async fn run(args: ValidateArgs, legacy_mode: bool) -> anyhow::Result<i32> {
    // 1. Load Config
    let cfg = match load_config(&args.config, legacy_mode, true) {
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
                args.output.as_deref(),
            );
            return Ok(2);
        }
    };

    let resolver = PathResolver::new(&args.config);

    // 2. Prepare Options
    // In validate command, we usually validte what IS passed.
    let opts = ValidateOptions {
        trace_file: args.trace_file,
        baseline_file: args.baseline,
        replay_strict: args.replay_strict,
    };

    // 3. Run Validation
    let report = validate(&cfg, &opts, &resolver).await?;

    // 4. Print Report
    // 4. Print Report / Export
    print_report(&report, &args.format, args.output.as_deref());

    // 5. Determine Exit Code
    // Any error severity -> 2. Warnings only -> 0.
    if report.diagnostics.iter().any(|d| d.severity == "error") {
        Ok(2)
    } else {
        Ok(0)
    }
}

fn print_report(report: &ValidateReport, format: &str, output: Option<&std::path::Path>) {
    if format == "sarif" {
        use assay_core::report::sarif::write_sarif_diagnostics;
        // Default to "results.sarif" if not specified? Or stdout?
        // Roadmap says "--output results.sarif".
        // If no output, we should probably output to stdout for piping.
        // But `write_sarif_diagnostics` takes a path.
        // Let's assume file output if provided, else output to stdout using a robust way.

        if let Some(path) = output {
             if let Err(e) = write_sarif_diagnostics("assay", &report.diagnostics, path) {
                 eprintln!("Failed to write SARIF: {}", e);
             } else {
                 eprintln!("SARIF report written to {}", path.display());
             }
        } else {
             // Hack: write to temp file then print? No, that's slow.
             // Ideally we refactor `sarif.rs` to return String.
             // For P0 MVP, let's write to "assay-report.sarif" by default and warn user.
             let default_path = std::path::Path::new("assay-report.sarif");
             if let Err(e) = write_sarif_diagnostics("assay", &report.diagnostics, default_path) {
                 eprintln!("Failed to write SARIF: {}", e);
             } else {
                 // SARIF users expect silent stdout usually, but if we behave like a linter...
                 println!("{}", default_path.display()); // Print filename for unix piping? No.
                 eprintln!("SARIF report written to {} (use --output to specify)", default_path.display());
             }
        }
    } else if format == "json" {
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

        let output_json = json!({
            "schema_version": 1,
            "ok": ok,
            "errors": errors,
            "warnings": warnings,
            "summary": {
                // We could populate this from the report struct if we added it
                "diagnostic_count": report.diagnostics.len()
            }
        });

        if let Some(path) = output {
             std::fs::write(path, serde_json::to_string_pretty(&output_json).unwrap()).unwrap();
        } else {
             println!("{}", serde_json::to_string_pretty(&output_json).unwrap());
        }
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
