use crate::cli::args::{SimArgs, SimRunArgs, SimSub, SoakArgs, SoakDecisionPolicy, SoakMode};
use crate::exit_codes::EXIT_CONFIG_ERROR;
use anyhow::{Context, Result};
use assay_evidence::lint::engine::{lint_bundle_with_options, LintOptions};
use assay_evidence::lint::packs::load_packs;
use assay_evidence::lint::Severity;
use assay_evidence::{VerifyLimits, VerifyLimitsOverrides};
use assay_sim::{
    run_suite,
    soak::{VAR_SRC_DETERMINISTIC_REPEAT, VAR_SRC_RUN_TRAJECTORIES},
    tier_default_limits, DecisionPolicy, PackRef, RunResult, SoakLimits, SoakReport, SoakResults,
    SuiteConfig, SuiteTier,
};
use std::collections::BTreeMap;
use std::fs;
use std::io::Cursor;
use std::time::Instant;

/// Default pack when --pack is omitted (ADR-023).
const DEFAULT_PACK: &str = "cicd-starter";

pub fn run(args: SimArgs) -> Result<i32> {
    match args.cmd {
        SimSub::Run(a) => cmd_run(a),
        SimSub::Soak(a) => cmd_soak(a),
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

    // Save JSON report if requested (before any early return so budget-exceeded runs still produce output)
    if let Some(path) = args.report {
        let json = serde_json::to_string_pretty(&report)?;
        std::fs::write(&path, json).context("failed to write report")?;
        println!("Report saved to {}", path.display());
    }

    // Bypass (exit 1) takes precedence over time budget exceeded (exit 2)
    if report.summary.bypassed > 0 {
        eprintln!(
            "\n❌ SECURITY REGRESSION: {} attacks bypassed verification!",
            report.summary.bypassed
        );
        return Ok(1);
    }

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

    println!("\n✅ All attacks blocked.");
    Ok(0)
}

fn cmd_soak(args: SoakArgs) -> Result<i32> {
    if args.iterations == 0 {
        eprintln!("Config error: --iterations must be > 0");
        std::process::exit(EXIT_CONFIG_ERROR);
    }

    match args.mode {
        SoakMode::Run => {
            eprintln!(
                "Config error: --mode=run is not implemented yet (E1.1). This command will run your agent N times and lint each produced bundle. Track: ADR-025 E1.2."
            );
            std::process::exit(EXIT_CONFIG_ERROR);
        }
        SoakMode::Artifact => {
            if args.target.is_none() {
                eprintln!(
                    "Config error: --target is required when --mode=artifact (or omit --mode for default)."
                );
                std::process::exit(EXIT_CONFIG_ERROR);
            }
        }
    }

    let target = args
        .target
        .as_ref()
        .expect("target required in artifact mode");

    let severity = match args.decision_policy {
        SoakDecisionPolicy::Error => Severity::Error,
        SoakDecisionPolicy::Warning => Severity::Warn,
        SoakDecisionPolicy::Info => Severity::Info,
    };

    let decision_policy_str = match args.decision_policy {
        SoakDecisionPolicy::Error => "error",
        SoakDecisionPolicy::Warning => "warning",
        SoakDecisionPolicy::Info => "info",
    };

    let seed = args.seed.unwrap_or_else(|| {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    });

    let pack_refs: Vec<String> = args.pack.unwrap_or_else(|| vec![DEFAULT_PACK.to_string()]);

    let packs = match load_packs(&pack_refs) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Pack loading failed: {}", e);
            std::process::exit(3);
        }
    };

    let bundle_bytes =
        fs::read(target).with_context(|| format!("failed to read bundle {}", target.display()))?;

    let limits = VerifyLimits::default();
    let options = LintOptions {
        packs: packs.clone(),
        max_results: Some(5000),
        bundle_path: Some(target.display().to_string()),
    };

    let mut passes = 0u32;
    let mut failures = 0u32;
    let mut infra_errors = 0u32;
    let mut first_policy_failure_at: Option<u32> = None;
    let mut first_infra_error_at: Option<u32> = None;
    let mut violations_by_rule: BTreeMap<String, u32> = BTreeMap::new();
    let mut run_results: Vec<RunResult> = Vec::with_capacity(args.iterations as usize);
    let start = Instant::now();

    let mut time_budget_exceeded_count = 0u32;

    for i in 1..=args.iterations {
        if args.time_budget > 0 && start.elapsed().as_secs() >= args.time_budget {
            let remaining = args.iterations - i + 1;
            infra_errors += remaining;
            time_budget_exceeded_count = remaining;
            if first_infra_error_at.is_none() {
                first_infra_error_at = Some(i);
            }
            break;
        }

        let run_start = Instant::now();
        match lint_bundle_with_options(Cursor::new(&bundle_bytes), limits, options.clone()) {
            Ok(result) => {
                let duration_ms = run_start.elapsed().as_millis() as u64;
                let failed = result.report.has_findings_at_or_above(&severity);
                let violated: Vec<String> = result
                    .report
                    .findings
                    .iter()
                    .filter(|f| match &severity {
                        Severity::Error => f.severity == Severity::Error,
                        Severity::Warn => {
                            f.severity == Severity::Error || f.severity == Severity::Warn
                        }
                        Severity::Info => true,
                    })
                    .map(|f| f.rule_id.clone())
                    .collect();

                for rid in &violated {
                    *violations_by_rule.entry(rid.clone()).or_insert(0) += 1;
                }

                if failed {
                    failures += 1;
                    if first_policy_failure_at.is_none() {
                        first_policy_failure_at = Some(i);
                    }
                    run_results.push(RunResult {
                        index: i,
                        status: "fail".into(),
                        duration_ms,
                        violated_rules: Some(violated),
                        infra_error_kind: None,
                        infra_error_message: None,
                    });
                } else {
                    passes += 1;
                    run_results.push(RunResult {
                        index: i,
                        status: "pass".into(),
                        duration_ms,
                        violated_rules: None,
                        infra_error_kind: None,
                        infra_error_message: None,
                    });
                }
            }
            Err(e) => {
                infra_errors += 1;
                if first_infra_error_at.is_none() {
                    first_infra_error_at = Some(i);
                }
                run_results.push(RunResult {
                    index: i,
                    status: "infra_error".into(),
                    duration_ms: run_start.elapsed().as_millis() as u64,
                    violated_rules: None,
                    infra_error_kind: Some("verification_failed".into()),
                    infra_error_message: Some(e.to_string()),
                });
            }
        }
    }

    let completed_runs = passes + failures;
    let pass_rate = if completed_runs > 0 {
        passes as f64 / completed_runs as f64
    } else {
        0.0
    };
    let pass_all = infra_errors == 0 && failures == 0;

    let pack_refs: Vec<PackRef> = packs
        .iter()
        .map(|p| PackRef {
            name: p.definition.name.clone(),
            version: p.definition.version.clone(),
            digest: p.digest.clone(),
            kind: Some(p.definition.kind.to_string()),
            source: Some(p.source.to_string()),
        })
        .collect();

    let (report_mode, variation_source) = match args.mode {
        SoakMode::Artifact => ("artifact", VAR_SRC_DETERMINISTIC_REPEAT),
        SoakMode::Run => ("run", VAR_SRC_RUN_TRAJECTORIES), // unreachable (stub exits earlier)
    };

    let report = SoakReport {
        schema_version: "soak-report-v1".into(),
        mode: "soak".into(),
        soak_mode: report_mode.into(),
        variation_source: variation_source.into(),
        generated_at: Some(chrono::Utc::now().to_rfc3339()),
        assay_version: Some(env!("CARGO_PKG_VERSION").to_string()),
        suite: None,
        iterations: args.iterations,
        seed,
        seed_strategy: Some("fixed".into()),
        time_budget_secs: args.time_budget,
        time_budget_scope: "soak".into(),
        limits: SoakLimits::from(limits),
        packs: pack_refs,
        decision_policy: DecisionPolicy {
            pass_on_severity_at_or_above: decision_policy_str.into(),
            stop_on_first_failure: None,
            max_failures: None,
        },
        results: SoakResults {
            runs: args.iterations,
            passes,
            failures,
            infra_errors,
            pass_rate,
            pass_all,
            first_policy_failure_at,
            first_infra_error_at,
            violations_by_rule: if violations_by_rule.is_empty() {
                None
            } else {
                Some(violations_by_rule.clone())
            },
            infra_errors_by_kind: if infra_errors > 0 {
                let mut m = BTreeMap::new();
                if time_budget_exceeded_count > 0 {
                    m.insert("time_budget_exceeded".into(), time_budget_exceeded_count);
                }
                let verify_failures = infra_errors - time_budget_exceeded_count;
                if verify_failures > 0 {
                    m.insert("verification_failed".into(), verify_failures);
                }
                Some(m)
            } else {
                None
            },
        },
        runs: Some(run_results),
    };

    let report_to_stdout = args
        .report
        .as_ref()
        .map(|p| p.as_os_str() == "-")
        .unwrap_or(false);

    let log: &dyn Fn(&str) = if report_to_stdout {
        &|s: &str| eprintln!("{}", s)
    } else {
        &|s: &str| println!("{}", s)
    };

    log("Assay Soak (ADR-025)");
    log("===================");
    log(&format!("Mode:       {}", report_mode));
    log(&format!("Variation:  {}", variation_source));
    log(&format!("Iterations: {}", args.iterations));
    log(&format!("Target:     {}", target.display()));
    log(&format!(
        "Threshold:  {} (findings >= this count as failure)",
        decision_policy_str
    ));
    if args.mode == SoakMode::Artifact && !args.quiet {
        log("Note: --mode=artifact repeats lint on a fixed bundle. This measures policy determinism/report stability, not agent variance/drift. Use --mode=run --run-cmd <cmd> for pass^k under variance.");
    }
    log(&format!("Seed:       {}", seed));
    log(&format!("Pass rate: {:.1}%", pass_rate * 100.0));
    if let Some(idx) = first_policy_failure_at {
        log(&format!("First policy failure at run: {}", idx));
    }
    if let Some(idx) = first_infra_error_at {
        log(&format!("First infra error at run: {}", idx));
    }
    let top_rules: Vec<_> = {
        let mut v: Vec<_> = violations_by_rule.iter().collect();
        v.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        v.into_iter()
            .take(3)
            .map(|(k, v)| (k.clone(), *v))
            .collect()
    };
    if !top_rules.is_empty() {
        log(&format!("Top violated rules: {:?}", top_rules));
    }
    log("");

    if let Some(ref path) = args.report {
        let json = serde_json::to_string_pretty(&report)?;
        if path.as_os_str() == "-" {
            println!("{}", json);
        } else {
            std::fs::write(path, json).context("failed to write report")?;
            if !report_to_stdout {
                log(&format!("Report saved to {}", path.display()));
            }
        }
    }

    if infra_errors > 0 {
        let msg = if time_budget_exceeded_count > 0 {
            format!(
                "\nInfra error: time budget exceeded after {} runs (skipped {} runs)",
                args.iterations - time_budget_exceeded_count,
                time_budget_exceeded_count
            )
        } else {
            format!("\nInfra error: {} runs failed verification", infra_errors)
        };
        eprintln!("{}", msg);
        return Ok(2);
    }
    if failures > 0 {
        eprintln!(
            "\nPolicy fail: {} runs had findings >= {}",
            failures, decision_policy_str
        );
        return Ok(1);
    }

    log(&format!("\nAll {} runs passed.", args.iterations));
    Ok(0)
}
