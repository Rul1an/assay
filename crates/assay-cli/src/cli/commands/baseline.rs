use crate::cli::args::BaselineReportArgs;
use anyhow::Context;
use assay_core::baseline::report::report_from_db;
use assay_core::storage::Store;
use std::fs;

pub fn cmd_baseline_report(args: BaselineReportArgs) -> anyhow::Result<()> {
    let store = Store::open(&args.db)?;
    store.init_schema()?;
    let report = report_from_db(&store, &args.suite, args.last)?;

    if args.format == "json" && args.out.extension().is_none_or(|ext| ext == "json") {
        let json = serde_json::to_string_pretty(&report)?;
        std::fs::write(&args.out, json)?;
        eprintln!("wrote {}", args.out.display());
    } else if args.format == "md" || args.out.extension().is_some_and(|ext| ext == "md") {
        let md = generate_markdown(&report);
        fs::write(&args.out, md)?;
        eprintln!("wrote {}", args.out.display());
    } else {
        // Default to JSON
        let json = serde_json::to_string_pretty(&report)?;
        std::fs::write(&args.out, json)?;
        eprintln!("wrote {}", args.out.display());
    }

    Ok(())
}

pub fn cmd_baseline_record(args: crate::cli::args::BaselineRecordArgs) -> anyhow::Result<()> {
    let cfg = assay_core::config::load_config(&args.config, false, false)?;
    let suite = args.suite.unwrap_or(cfg.suite.clone());

    let store = Store::open(&args.db)?;
    // We don't necessarily need full init_schema if just reading, but safer to call it
    store.init_schema()?;

    let run_id = if let Some(id_str) = args.run_id {
        // Try parsing as integer
        id_str.parse::<i64>().context("invalid run_id")?
    } else {
        store
            .get_latest_run_id(&suite)?
            .context(format!("no runs found for suite '{}'", suite))?
    };

    let results = store.fetch_results_for_run(run_id)?;
    if results.is_empty() {
        anyhow::bail!("run {} has no results", run_id);
    }

    // Flatten results into Baseline entries (scores only)
    let mut entries = Vec::new();
    for r in results {
        if let Some(metrics) = r.details.get("metrics").and_then(|v| v.as_object()) {
            for (metric_name, m_val) in metrics {
                if let Some(score) = m_val.get("score").and_then(|s| s.as_f64()) {
                    entries.push(assay_core::baseline::BaselineEntry {
                        test_id: r.test_id.clone(),
                        metric: metric_name.clone(),
                        score,
                        meta: None,
                    });
                }
            }
        } else if let Some(score) = r.score {
            // If no granular metrics, use main score if present
            entries.push(assay_core::baseline::BaselineEntry {
                test_id: r.test_id.clone(),
                metric: "score".to_string(),
                score,
                meta: None,
            });
        }
    }

    let git_info = capture_git_info();

    let baseline = assay_core::baseline::Baseline {
        schema_version: 1,
        suite: suite.clone(),
        assay_version: env!("CARGO_PKG_VERSION").to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        config_fingerprint: assay_core::baseline::compute_config_fingerprint(&args.config),
        git_info,
        entries,
    };

    baseline.save(&args.out)?;
    eprintln!(
        "Recorded baseline for run {} (suite: {}) to {}",
        run_id,
        suite,
        args.out.display()
    );
    Ok(())
}

pub fn cmd_baseline_check(args: crate::cli::args::BaselineCheckArgs) -> anyhow::Result<()> {
    let baseline = assay_core::baseline::Baseline::load(&args.baseline)?;

    let cfg = assay_core::config::load_config(&args.config, false, false)?;
    let suite = args.suite.unwrap_or(cfg.suite.clone());

    if let Err(e) = baseline.validate(
        &suite,
        &assay_core::baseline::compute_config_fingerprint(&args.config),
    ) {
        eprintln!("warning: {}", e);
    }

    let store = Store::open(&args.db)?;
    let run_id = if let Some(id_str) = args.run_id {
        id_str.parse::<i64>().context("invalid run_id")?
    } else {
        store
            .get_latest_run_id(&suite)?
            .context(format!("no runs found for suite '{}'", suite))?
    };

    let results = store.fetch_results_for_run(run_id)?;
    if results.is_empty() {
        anyhow::bail!("run {} has no results", run_id);
    }

    // Convert current run to Baseline struct for diffing
    let mut entries = Vec::new();
    for r in results {
        if let Some(metrics) = r.details.get("metrics").and_then(|v| v.as_object()) {
            for (metric_name, m_val) in metrics {
                if let Some(score) = m_val.get("score").and_then(|s| s.as_f64()) {
                    entries.push(assay_core::baseline::BaselineEntry {
                        test_id: r.test_id.clone(),
                        metric: metric_name.clone(),
                        score,
                        meta: None,
                    });
                }
            }
        } else if let Some(score) = r.score {
            entries.push(assay_core::baseline::BaselineEntry {
                test_id: r.test_id.clone(),
                metric: "score".to_string(),
                score,
                meta: None,
            });
        }
    }

    // Temporary baseline for current state
    let candidate = assay_core::baseline::Baseline {
        schema_version: 1,
        suite: suite.clone(),
        assay_version: env!("CARGO_PKG_VERSION").to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        config_fingerprint: "".to_string(), // irrelevant for diff
        git_info: None,
        entries,
    };

    let diff = baseline.diff(&candidate);

    if args.format == crate::cli::args::OutputFormat::Json {
        println!("{}", serde_json::to_string_pretty(&diff)?);
    } else {
        // Print Report
        println!("Baseline comparison against run {}", run_id);
        if !diff.regressions.is_empty() {
            println!("\n❌ REGRESSIONS ({}):", diff.regressions.len());
            for r in &diff.regressions {
                println!(
                    "  - {} metric '{}': {:.2} -> {:.2} ({:.2})",
                    r.test_id, r.metric, r.baseline_score, r.candidate_score, r.delta
                );
            }
        } else {
            println!("\n✅ No regressions.");
        }

        if !diff.improvements.is_empty() {
            println!("\n🎉 IMPROVEMENTS ({}):", diff.improvements.len());
            for i in &diff.improvements {
                println!(
                    "  - {} metric '{}': {:.2} -> {:.2} (+{:.2})",
                    i.test_id, i.metric, i.baseline_score, i.candidate_score, i.delta
                );
            }
        }

        if !diff.new_tests.is_empty() {
            println!("\n🆕 NEW TESTS/METRICS ({}):", diff.new_tests.len());
            for n in &diff.new_tests {
                println!("  - {}", n);
            }
        }

        if !diff.missing_tests.is_empty() {
            println!("\n⚠️ MISSING TESTS/METRICS ({}):", diff.missing_tests.len());
            for m in &diff.missing_tests {
                println!("  - {}", m);
            }
        }
    }

    if args.fail_on_regression && !diff.regressions.is_empty() {
        anyhow::bail!("Regression check failed");
    }

    Ok(())
}

pub fn capture_git_info() -> Option<assay_core::baseline::GitInfo> {
    // Try git command first
    if let Some(info) = capture_git_from_cmd() {
        return Some(info);
    }
    // Fallback to Env Vars (CI)
    capture_git_from_env()
}

fn capture_git_from_cmd() -> Option<assay_core::baseline::GitInfo> {
    use std::process::Command;

    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let commit = String::from_utf8_lossy(&output.stdout).trim().to_string();

    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;
    let branch = if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    };

    let output = Command::new("git")
        .args(["diff", "--quiet"])
        .output()
        .ok()?;
    let dirty = !output.status.success();

    let output = Command::new("git")
        .args(["show", "-s", "--format=%an"])
        .output()
        .ok()?;
    let author = if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    };

    let output = Command::new("git")
        .args(["show", "-s", "--format=%cI"])
        .output()
        .ok()?;
    let timestamp = if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    };

    Some(assay_core::baseline::GitInfo {
        commit,
        branch,
        dirty,
        author,
        timestamp,
    })
}

fn capture_git_from_env() -> Option<assay_core::baseline::GitInfo> {
    let commit = std::env::var("GITHUB_SHA")
        .or_else(|_| std::env::var("GIT_COMMIT"))
        .ok()?;

    let branch = std::env::var("GITHUB_REF_NAME")
        .or_else(|_| std::env::var("GIT_BRANCH"))
        .ok();

    Some(assay_core::baseline::GitInfo {
        commit,
        branch,
        dirty: false, // Assume CI is clean
        author: std::env::var("GITHUB_ACTOR").ok(),
        timestamp: None,
    })
}

fn generate_markdown(report: &assay_core::baseline::report::HygieneReport) -> String {
    let mut md = String::new();

    md.push_str(&format!("# Baseline Hygiene Report: {}\n\n", report.suite));
    md.push_str(&format!(
        "**Source**: `{}` | **Generated**: {} | **Window**: Last {} runs\n\n",
        report.source, report.generated_at, report.window.last_runs
    ));

    if !report.notes.is_empty() {
        md.push_str("### ⚠️ Notes\n");
        for note in &report.notes {
            md.push_str(&format!("- {}\n", note));
        }
        md.push('\n');
    }

    md.push_str("### Test Stability & Performance\n\n");
    md.push_str("| Test ID | N | Pass | Fail | Flaky | Skip | P90 Score (SemSim) | Top Issues |\n");
    md.push_str("|---|---|---|---|---|---|---|---|\n");

    for t in &report.tests {
        let semsim = t
            .scores
            .get("semantic_similarity_to")
            .map(|s| format!("{:.2}", s.p90))
            .unwrap_or_else(|| "-".to_string());

        let issues: Vec<String> = t
            .top_reasons
            .iter()
            .map(|r| format!("{}: {} ({})", r.kind, r.value, r.count))
            .collect();
        let issues_str = if issues.is_empty() {
            "-".to_string()
        } else {
            issues.join("<br>")
        };

        md.push_str(&format!(
            "| `{}` | {} | {:.0}% | {:.0}% | {:.0}% | {:.0}% | {} | {} |\n",
            t.test_id,
            t.n,
            t.rates.pass * 100.0,
            t.rates.fail * 100.0,
            t.rates.flaky * 100.0,
            t.rates.skipped * 100.0,
            semsim,
            issues_str
        ));
    }

    md
}
