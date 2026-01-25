use assay_core::config::{load_config, path_resolver::PathResolver};

use crate::cli::args::DoctorArgs;
use crate::diagnostics;
use crate::diagnostics::format::format_text;

pub async fn run(args: DoctorArgs, legacy_mode: bool) -> anyhow::Result<i32> {
    // 1. Unified System Diagnostics
    let report = diagnostics::probe_system();

    // 2. Data/Config Diagnostics via Core
    let (target_path, explicit) = match args.config {
        Some(p) => (p, true),
        None => (std::path::PathBuf::from("eval.yaml"), false),
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
        // Construct composite JSON
        // We serialize the system report and inject other fields
        let mut json_out = serde_json::to_value(&report)?;

        if let Some(obj) = json_out.as_object_mut() {
            obj.insert("generated_at".to_string(), serde_json::json!(timestamp));

            if let Some(err) = cfg_err {
                // If config error, add to issues/diagnostics
                obj.insert(
                    "config_error".to_string(),
                    serde_json::json!({
                        "message": err.to_string(),
                        "code": "E_CFG_PARSE"
                    }),
                );
                // We return 1 if JSON requested and config bad? Or 0 with error details?
                // Previous behavior was 1. adhering to expected behavior.
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

    // Deep diagnostics (Config + Data) only if config is valid
    if let Some(e) = cfg_err {
        println!("\nConfig Status: FAILED");
        println!("  File:     {}", target_path.display());
        println!("  Error:    {}\n", e);
        return Ok(1);
    }

    // ... code for deep diagnostics ...
    if let Some(c) = cfg {
        // Existing deep diagnostics logic preserved
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
    } else {
        println!("\nPolicy Check: SKIPPED (No config found; run inside project or use --config)");
    }

    Ok(0)
}
