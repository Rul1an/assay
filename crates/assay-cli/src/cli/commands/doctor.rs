use assay_core::config::{load_config, path_resolver::PathResolver};

use crate::cli::args::DoctorArgs;

pub async fn run(args: DoctorArgs, legacy_mode: bool) -> anyhow::Result<i32> {
    let cfg = load_config(&args.config, legacy_mode)
        .map_err(|e| anyhow::anyhow!("config error: {}", e))?;
    let resolver = PathResolver::new(&args.config);

    let opts = assay_core::doctor::DoctorOptions {
        config_path: args.config.clone(),
        trace_file: args.trace_file.clone(),
        baseline_file: args.baseline.clone(),
        db_path: args.db.clone(),
        replay_strict: args.replay_strict,
    };

    let report = assay_core::doctor::doctor(&cfg, &opts, &resolver).await?;

    let rendered = if args.format == "json" {
        serde_json::to_string_pretty(&report)?
    } else {
        // minimal human formatting (keep it short; diagnostics already have format_terminal)
        let mut s = String::new();
        s.push_str(&format!("Assay Doctor (v{})\n", report.assay_version));
        s.push_str(&format!(
            "Suite: {}\n",
            report
                .config
                .as_ref()
                .map(|c| c.suite.as_str())
                .unwrap_or("<unknown>")
        ));
        s.push_str(&format!("Diagnostics: {}\n", report.diagnostics.len()));
        s.push_str("\nNext actions:\n");
        for a in &report.suggested_actions {
            s.push_str(&format!("- {}\n", a.title));
        }
        s
    };

    if let Some(p) = args.out {
        std::fs::write(&p, rendered)?;
        eprintln!("wrote file: {}", p.display());
    } else if args.format == "json" {
        println!("{}", rendered);
    } else {
        eprintln!("{}", rendered);
    }

    Ok(0)
}
