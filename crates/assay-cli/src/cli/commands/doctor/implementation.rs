use std::path::PathBuf;

use assay_core::config::{load_config, path_resolver::PathResolver};

use crate::cli::args::DoctorArgs;
use crate::cli::helpers::normalize_severity;
use crate::diagnostics;
use crate::diagnostics::format::format_text;

use super::fixes::run_doctor_fix;
use super::parse_error::try_fix_parse_error;

pub async fn run(args: DoctorArgs, legacy_mode: bool) -> anyhow::Result<i32> {
    if args.fix && args.format == "json" {
        eprintln!("doctor --fix currently supports text output only; use --format text");
        return Ok(1);
    }
    if (args.yes || args.dry_run) && !args.fix {
        eprintln!("doctor: --yes/--dry-run require --fix");
        return Ok(1);
    }

    // 1. Unified System Diagnostics
    let report = diagnostics::probe_system();

    // 2. Data/Config Diagnostics via Core
    let (target_path, explicit) = match args.config.clone() {
        Some(p) => (p, true),
        None => (PathBuf::from("eval.yaml"), false),
    };

    let (cfg, cfg_err) = if explicit || target_path.exists() {
        match load_config(&target_path, legacy_mode, false) {
            Ok(c) => (Some(c), None),
            Err(e) => (None, Some(e.to_string())),
        }
    } else {
        (None, None)
    };

    if args.format == "json" {
        let timestamp = chrono::Utc::now().to_rfc3339();
        let mut json_out = serde_json::to_value(&report)?;

        if let Some(obj) = json_out.as_object_mut() {
            obj.insert("generated_at".to_string(), serde_json::json!(timestamp));

            if let Some(err) = cfg_err {
                obj.insert(
                    "config_error".to_string(),
                    serde_json::json!({
                        "message": err.to_string(),
                        "code": "E_CFG_PARSE"
                    }),
                );
                println!("{}", serde_json::to_string_pretty(&json_out)?);
                return Ok(1);
            }

            if let Some(c) = &cfg {
                let resolver = PathResolver::new(&target_path);
                let opts = assay_core::doctor::DoctorOptions {
                    config_path: target_path.clone(),
                    trace_file: args.trace_file.clone(),
                    baseline_file: args.baseline.clone(),
                    db_path: args.db.clone(),
                    replay_strict: args.replay_strict,
                };
                let core_report = assay_core::doctor::doctor(c, &opts, &resolver).await?;
                obj.insert(
                    "data_diagnostics".to_string(),
                    serde_json::to_value(&core_report.diagnostics)?,
                );
                obj.insert(
                    "data_suggestions".to_string(),
                    serde_json::to_value(&core_report.suggested_actions)?,
                );
            }
        }

        println!("{}", serde_json::to_string_pretty(&json_out)?);
        return Ok(0);
    }

    // Text Format
    let text_output = format_text(&report);
    println!("{}", text_output);

    if let Some(e) = cfg_err {
        println!("\nConfig Status: FAILED");
        println!("  File:     {}", target_path.display());
        println!("  Error:    {}\n", e);

        if args.fix {
            return try_fix_parse_error(&args, &target_path, &e, legacy_mode);
        }
        return Ok(1);
    }

    if let Some(c) = cfg {
        println!("\nPolicy Check:");
        println!("  Config:   {}", target_path.display());
        println!("  Suite:    {}", c.suite);

        let resolver = PathResolver::new(&target_path);
        let opts = assay_core::doctor::DoctorOptions {
            config_path: target_path.clone(),
            trace_file: args.trace_file.clone(),
            baseline_file: args.baseline.clone(),
            db_path: args.db.clone(),
            replay_strict: args.replay_strict,
        };
        let core_report = assay_core::doctor::doctor(&c, &opts, &resolver).await?;

        if !core_report.diagnostics.is_empty() {
            println!("  Issues:   {}", core_report.diagnostics.len());
            for d in &core_report.diagnostics {
                println!("    - [{}] {}", d.severity, d.message);
            }
        } else {
            println!("  Issues:   None (Clean)");
        }

        if args.fix {
            let fix_result =
                run_doctor_fix(&args, &target_path, &core_report.diagnostics, legacy_mode).await?;
            return Ok(fix_result);
        }

        let has_errors = core_report
            .diagnostics
            .iter()
            .any(|d| normalize_severity(&d.severity) == "error");
        return Ok(if has_errors { 1 } else { 0 });
    }

    println!("\nPolicy Check: SKIPPED (No config found; run inside project or use --config)");
    Ok(0)
}
