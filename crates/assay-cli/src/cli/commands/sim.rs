use crate::cli::args::{SimArgs, SimRunArgs, SimSub};
use anyhow::{Context, Result};
use assay_sim::{run_suite, SuiteConfig, SuiteTier};

pub fn run(args: SimArgs) -> Result<i32> {
    match args.cmd {
        SimSub::Run(a) => cmd_run(a),
    }
}

fn cmd_run(args: SimRunArgs) -> Result<i32> {
    let tier = match args.suite.to_lowercase().as_str() {
        "quick" => SuiteTier::Quick,
        "nightly" => SuiteTier::Nightly,
        "stress" => SuiteTier::Stress,
        _ => anyhow::bail!("unknown suite tier: {}", args.suite),
    };

    let seed = args.seed.unwrap_or_else(|| {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    });

    let config = SuiteConfig {
        tier,
        target_bundle: args.target.clone(),
        seed,
        verify_limits: None, // TODO: parse from args.limits if needed
    };

    println!("Assay Attack Simulation");
    println!("=======================");
    println!("Suite:  {:?}", config.tier);
    println!("Target: {}", config.target_bundle.display());
    println!("Seed:   {}", config.seed);
    println!();

    let report = run_suite(config).context("Simulation suite failed")?;

    // Print Results Table
    println!(
        "{:<30} {:<10} {:<10} {:<15}",
        "ATTACK/CHECK", "STATUS", "DUR(ms)", "ERROR_CODE"
    );
    println!("{:-<30} {:-<10} {:-<10} {:-<15}", "", "", "", "");

    for res in &report.results {
        let status_str = format!("{:?}", res.status);
        let error_code = res.error_code.as_deref().unwrap_or("-");
        println!(
            "{:<30} {:<10} {:<10} {:<15}",
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

    // Exit codes:
    // 0 = OK
    // 1 = Bypass Found
    // 2 = Infra error (handled by anyhow/Result)
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
