use crate::cli::args::{SimArgs, SimRunArgs, SimSub};
use crate::exit_codes::EXIT_CONFIG_ERROR;
use anyhow::{Context, Result};
use assay_evidence::{VerifyLimits, VerifyLimitsOverrides};
use assay_sim::{run_suite, tier_default_limits, SuiteConfig, SuiteTier};
use std::fs;

pub fn run(args: SimArgs) -> Result<i32> {
    match args.cmd {
        SimSub::Run(a) => cmd_run(a),
    }
}

/// Parse limits from CLI. Merge precedence: tier default → --limits → --limits-file.
fn parse_limits(args: &SimRunArgs) -> Result<VerifyLimits> {
    let mut defaults = tier_default_limits(args.suite.trim());
    if let Some(ref s) = args.limits {
        let overrides = if s.starts_with('@') {
            let path = s.trim_start_matches('@').trim();
            if path.is_empty() {
                anyhow::bail!("--limits @path: path cannot be empty");
            }
            let content = fs::read_to_string(path)
                .with_context(|| format!("limits file not found: {}", path))?;
            serde_json::from_str::<VerifyLimitsOverrides>(&content)
                .with_context(|| format!("invalid limits JSON in {}", path))?
        } else {
            serde_json::from_str::<VerifyLimitsOverrides>(s)
                .context("invalid --limits JSON (use --limits-file or --limits @path for file)")?
        };
        defaults = defaults.apply(overrides);
    }
    if let Some(ref p) = args.limits_file {
        let content = fs::read_to_string(p)
            .with_context(|| format!("limits file not found: {}", p.display()))?;
        let overrides = serde_json::from_str::<VerifyLimitsOverrides>(&content)
            .with_context(|| format!("invalid limits JSON in {}", p.display()))?;
        defaults = defaults.apply(overrides);
    }
    Ok(defaults)
}

fn cmd_run(args: SimRunArgs) -> Result<i32> {
    if args.time_budget == 0 {
        eprintln!("Config error: --time-budget must be > 0");
        std::process::exit(EXIT_CONFIG_ERROR);
    }

    let tier = match args.suite.to_lowercase().as_str() {
        "quick" => SuiteTier::Quick,
        "nightly" => SuiteTier::Nightly,
        "stress" => SuiteTier::Stress,
        "chaos" => SuiteTier::Chaos,
        _ => {
            eprintln!("Config error: unknown suite tier: {}", args.suite);
            std::process::exit(EXIT_CONFIG_ERROR);
        }
    };

    let verify_limits = match parse_limits(&args) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Config error: {}", e);
            std::process::exit(EXIT_CONFIG_ERROR);
        }
    };

    if args.print_config {
        println!("Effective limits:");
        println!("  max_bundle_bytes: {}", verify_limits.max_bundle_bytes);
        println!("  max_decode_bytes: {}", verify_limits.max_decode_bytes);
        println!("  time_budget: {}s", args.time_budget);
        return Ok(0);
    }

    let target = args
        .target
        .as_ref()
        .expect("clap ensures target required unless --print-config");

    let seed = args.seed.unwrap_or_else(|| {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    });

    let config = SuiteConfig {
        tier,
        target_bundle: target.clone(),
        seed,
        verify_limits: Some(verify_limits),
        time_budget_secs: args.time_budget,
    };

    println!("Assay Attack Simulation");
    println!("=======================");
    println!("Suite:  {:?}", config.tier);
    println!("Target: {}", config.target_bundle.display());
    println!("Seed:   {}", config.seed);
    println!();

    let report = run_suite(config).context("Simulation suite failed")?;

    // Exit 2 when time budget exceeded (ADR-024)
    if report.time_budget_exceeded {
        let skipped = if report.skipped_phases.is_empty() {
            "(none)".to_string()
        } else {
            report.skipped_phases.join(", ")
        };
        eprintln!("\n⏱ Time budget exceeded. Skipped: {}", skipped);
        return Ok(2);
    }

    // Print Results Table
    println!(
        "{:<35} {:<10} {:<10} {:<15}",
        "ATTACK/CHECK", "STATUS", "DUR(ms)", "ERROR_CODE"
    );
    println!("{:-<35} {:-<10} {:-<10} {:-<15}", "", "", "", "");

    for res in &report.results {
        let status_str = format!("{:?}", res.status);
        let error_code = res.error_code.as_deref().unwrap_or("-");
        println!(
            "{:<35} {:<10} {:<10} {:<15}",
            res.name, status_str, res.duration_ms, error_code
        );
    }

    println!();
    println!(
        "SUMMARY: blocked={} passed={} bypassed={}",
        report.summary.blocked, report.summary.passed, report.summary.bypassed
    );

    // Save JSON report if requested
    if let Some(path) = args.report {
        let json = serde_json::to_string_pretty(&report)?;
        std::fs::write(&path, json).context("failed to write report")?;
        println!("Report saved to {}", path.display());
    }

    if report.summary.bypassed > 0 {
        eprintln!(
            "\n❌ SECURITY REGRESSION: {} attacks bypassed verification!",
            report.summary.bypassed
        );
        return Ok(1);
    }

    println!("\n✅ All attacks blocked.");
    Ok(0)
}
