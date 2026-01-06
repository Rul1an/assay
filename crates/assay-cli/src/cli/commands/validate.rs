use assay_core::config::{load_config, path_resolver::PathResolver};
use assay_core::errors::diagnostic::Diagnostic;
use assay_core::validate::{validate, ValidateOptions, ValidateReport};
use serde_json::json;

use crate::cli::args::{ValidateArgs, ValidateOutputFormat};
use crate::cli::commands::exit_codes;

pub async fn run(args: ValidateArgs, legacy_mode: bool) -> anyhow::Result<i32> {
    // 1. Load Config
    let cfg = match load_config(&args.config, legacy_mode, true) {
        Ok(c) => c,
        Err(e) => {
            let diag = Diagnostic::new(
                assay_core::errors::diagnostic::codes::E_CFG_PARSE,
                format!("Failed to parse config: {}", e),
            )
            .with_source("config")
            .with_context(json!({ "file": args.config }));

            let report = ValidateReport {
                diagnostics: vec![diag],
            };

            let exit_code = exit_codes::CONFIG_ERROR;
            print_report(&report, &args, exit_code)?;
            return Ok(exit_code);
        }
    };

    let resolver = PathResolver::new(&args.config);

    // 2. Prepare Options
    let opts = ValidateOptions {
        trace_file: args.trace_file.clone(),
        baseline_file: args.baseline.clone(),
        replay_strict: args.replay_strict,
    };

    // 3. Run Validation
    let report = validate(&cfg, &opts, &resolver).await?;

    // 4. Determine Exit Code
    let exit_code = decide_validate_exit(&report);

    // 5. Print Report / Export
    print_report(&report, &args, exit_code)?;

    Ok(exit_code)
}

fn decide_validate_exit(report: &ValidateReport) -> i32 {
    let has_error = report.diagnostics.iter().any(|d| d.severity == "error");
    if !has_error {
        return exit_codes::OK;
    }

    // Heuristic: E_CFG_*, E_PATH*, E_TRACE_SCHEMA* -> CONFIG_ERROR (2)
    // Else -> TEST_FAILED (1)
    let is_config_like = report.diagnostics.iter().any(|d| {
        d.severity == "error"
            && (
                d.code.starts_with("E_CFG_")
                    || d.code.starts_with("E_PATH_")
                    || d.code.starts_with("E_TRACE_SCHEMA")
                    || d.code.starts_with("E_BASE_MISMATCH")
                // Arguably test fail, but usually config/env issue
            )
    });

    if is_config_like {
        exit_codes::CONFIG_ERROR
    } else {
        exit_codes::TEST_FAILED
    }
}

fn print_report(
    report: &ValidateReport,
    args: &ValidateArgs,
    exit_code: i32,
) -> anyhow::Result<()> {
    match args.format {
        ValidateOutputFormat::Sarif => {
            let doc = assay_core::report::sarif::build_sarif_diagnostics(
                "assay",
                &report.diagnostics,
                Some(exit_code),
            );
            let s = serde_json::to_string_pretty(&doc)?;

            if let Some(path) = &args.output {
                std::fs::write(path, s)
                    .map_err(|e| anyhow::anyhow!("failed to write {}: {}", path.display(), e))?;
                eprintln!("SARIF report written to {}", path.display());
            } else {
                println!("{}", s);
            }
        }
        ValidateOutputFormat::Json => {
            let output_json = build_validate_json(report, args, exit_code);
            let s = serde_json::to_string_pretty(&output_json)?;

            if let Some(path) = &args.output {
                std::fs::write(path, s)
                    .map_err(|e| anyhow::anyhow!("failed to write {}: {}", path.display(), e))?;
            } else {
                println!("{}", s);
            }
        }
        ValidateOutputFormat::Text => {
            // Text format is always printed to stderr (human-readable)
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

    Ok(())
}

use serde::Serialize;

fn build_validate_json(
    report: &ValidateReport,
    args: &ValidateArgs,
    exit_code: i32,
) -> serde_json::Value {
    let mut diags: Vec<&Diagnostic> = report.diagnostics.iter().collect();

    // Deterministic sort: severity_rank > code > message > file
    diags.sort_by(|a, b| {
        let af = a.context.get("file").and_then(|v| v.as_str()).unwrap_or("");
        let bf = b.context.get("file").and_then(|v| v.as_str()).unwrap_or("");

        let sa = normalize_severity(a.severity.as_str());
        let sb = normalize_severity(b.severity.as_str());

        (severity_rank(sa), a.code.as_str(), a.message.as_str(), af).cmp(&(
            severity_rank(sb),
            b.code.as_str(),
            b.message.as_str(),
            bf,
        ))
    });

    let diag_views: Vec<DiagView<'_>> = diags.iter().map(|d| DiagView::from(*d)).collect();

    let error_count = diag_views.iter().filter(|d| d.severity == "error").count();
    let warn_count = diag_views.iter().filter(|d| d.severity == "warn").count();
    let note_count = diag_views.len() - error_count - warn_count;

    let mut args_list: Vec<String> = vec![
        "--config".into(),
        args.config.display().to_string(),
        "--format".into(),
        match args.format {
            ValidateOutputFormat::Text => "text",
            ValidateOutputFormat::Json => "json",
            ValidateOutputFormat::Sarif => "sarif",
        }
        .into(),
    ];

    if let Some(trace) = &args.trace_file {
        args_list.push("--trace-file".into());
        args_list.push(trace.display().to_string());
    }
    if let Some(baseline) = &args.baseline {
        args_list.push("--baseline".into());
        args_list.push(baseline.display().to_string());
    }
    if args.replay_strict {
        args_list.push("--replay-strict".into());
    }
    if let Some(output) = &args.output {
        args_list.push("--output".into());
        args_list.push(output.display().to_string());
    }

    json!({
        "schema_version": 1,
        "ok": error_count == 0,
        "exit_code": exit_code,

        "tool": {
            "name": "assay",
            "version": env!("CARGO_PKG_VERSION")
        },

        "command": {
            "name": "validate",
            "args": args_list,
            "config_file": args.config.display().to_string(),
            "trace_file": args.trace_file.as_ref().map(|p| p.display().to_string()),
            "baseline_file": args.baseline.as_ref().map(|p| p.display().to_string())
        },

        "diagnostics": diag_views,

        // Agentic defaults (always present)
        "suggested_actions": [],
        "suggested_patches": [],

        "summary": {
            "diagnostic_count": diag_views.len(),
            "error_count": error_count,
            "warn_count": warn_count,
            "note_count": note_count,
            "replay_strict": args.replay_strict
        }
    })
}

fn severity_rank(s: &str) -> u8 {
    match s {
        "error" => 0,
        "warn" => 1,
        "note" => 2,
        _ => 3,
    }
}

fn normalize_severity(s: &str) -> &'static str {
    match s {
        "error" | "ERROR" => "error",
        "warn" | "warning" | "WARN" | "WARNING" => "warn",
        "note" | "info" | "INFO" => "note",
        _ => "note",
    }
}

#[derive(Serialize)]
struct DiagView<'a> {
    code: &'a str,
    severity: &'static str,
    source: &'a str,
    message: &'a str,
    context: &'a serde_json::Value,
    fix_steps: &'a Vec<String>,
}

impl<'a> From<&'a Diagnostic> for DiagView<'a> {
    fn from(d: &'a Diagnostic) -> Self {
        Self {
            code: d.code.as_str(),
            severity: normalize_severity(d.severity.as_str()),
            source: d.source.as_str(),
            message: d.message.as_str(),
            context: &d.context,
            fix_steps: &d.fix_steps,
        }
    }
}
