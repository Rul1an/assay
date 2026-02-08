use super::pipeline::PipelineTimings;
use super::run_output::{export_baseline, summary_from_outcome, write_run_json_minimal};
use crate::exit_codes::{ExitCodeVersion, ReasonCode, RunOutcome};
use std::path::{Path, PathBuf};

pub(crate) fn write_error_artifacts(
    reason: ReasonCode,
    message: String,
    version: ExitCodeVersion,
    verify_enabled: bool,
    run_json_path: &Path,
) -> anyhow::Result<i32> {
    let mut o = RunOutcome::from_reason(reason, Some(message), None);
    o.exit_code = reason.exit_code_for(version);
    if let Err(e) = write_run_json_minimal(&o, &run_json_path.to_path_buf()) {
        eprintln!("WARNING: failed to write run.json: {}", e);
    }

    let summary_path = run_json_path
        .parent()
        .map(|p| p.join("summary.json"))
        .unwrap_or_else(|| PathBuf::from("summary.json"));
    let summary = summary_from_outcome(&o, verify_enabled).with_seeds(None, None);
    if let Err(e) = assay_core::report::summary::write_summary(&summary, &summary_path) {
        eprintln!("WARNING: failed to write summary.json: {}", e);
    }
    Ok(o.exit_code)
}

pub(crate) fn build_summary_from_artifacts(
    outcome: &RunOutcome,
    verify_enabled: bool,
    artifacts: &assay_core::report::RunArtifacts,
    pipeline_timings: Option<&PipelineTimings>,
    report_ms: Option<u64>,
) -> assay_core::report::summary::Summary {
    let mut summary = summary_from_outcome(outcome, verify_enabled);
    let passed = artifacts
        .results
        .iter()
        .filter(|r| r.status.is_passing())
        .count();
    let failed = artifacts
        .results
        .iter()
        .filter(|r| r.status.is_blocking())
        .count();
    summary = summary.with_results(passed, failed, artifacts.results.len());
    summary = summary.with_seeds(artifacts.order_seed, None);
    if let Some(metrics) =
        assay_core::report::summary::judge_metrics_from_results(&artifacts.results)
    {
        summary = summary.with_judge_metrics(metrics);
    }
    let performance = build_performance_metrics(artifacts, pipeline_timings, report_ms);
    summary = summary.with_performance(performance);
    summary
}

fn build_performance_metrics(
    artifacts: &assay_core::report::RunArtifacts,
    pipeline_timings: Option<&PipelineTimings>,
    report_ms: Option<u64>,
) -> assay_core::report::summary::PerformanceMetrics {
    let (total_ms, ingest_ms, eval_ms) = match pipeline_timings {
        Some(t) => (
            t.total_ms.saturating_add(report_ms.unwrap_or(0)),
            t.ingest_ms,
            t.run_suite_ms,
        ),
        None => (report_ms.unwrap_or(0), None, None),
    };

    let cache_hit_rate = if artifacts.results.is_empty() {
        None
    } else {
        let cached = artifacts.results.iter().filter(|r| r.cached).count() as f64;
        Some(cached / artifacts.results.len() as f64)
    };

    let mut slowest: Vec<assay_core::report::summary::SlowestTest> = artifacts
        .results
        .iter()
        .filter_map(|row| {
            row.duration_ms
                .map(|d| assay_core::report::summary::SlowestTest {
                    test_id: row.test_id.clone(),
                    duration_ms: d,
                })
        })
        .collect();
    slowest.sort_by(|a, b| {
        b.duration_ms
            .cmp(&a.duration_ms)
            .then_with(|| a.test_id.cmp(&b.test_id))
    });
    slowest.truncate(5);
    let slowest_tests = if slowest.is_empty() {
        None
    } else {
        Some(slowest)
    };

    let phase_timings = Some(assay_core::report::summary::PhaseTimings {
        ingest_ms,
        eval_ms,
        judge_ms: None,
        report_ms,
    });

    assay_core::report::summary::PerformanceMetrics {
        total_duration_ms: total_ms,
        verify_ms: None,
        lint_ms: None,
        runner_clone_ms: artifacts.runner_clone_ms,
        runner_clone_count: None,
        profile_store_ms: None,
        run_id_memory_bytes: None,
        cache_hit_rate,
        slowest_tests,
        phase_timings,
    }
}

pub(crate) fn print_pipeline_summary(
    artifacts: &assay_core::report::RunArtifacts,
    explain_skip: bool,
    summary: &assay_core::report::summary::Summary,
) {
    assay_core::report::console::print_summary(&artifacts.results, explain_skip);
    assay_core::report::console::print_run_footer(
        Some(&summary.seeds),
        summary.judge_metrics.as_ref(),
    );
}

pub(crate) fn maybe_export_baseline(
    export_path: &Option<PathBuf>,
    config_path: &Path,
    cfg: &assay_core::model::EvalConfig,
    artifacts: &assay_core::report::RunArtifacts,
) {
    if let Some(path) = export_path {
        if let Err(e) = export_baseline(path, config_path, cfg, &artifacts.results) {
            eprintln!("Failed to export baseline: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assay_core::model::{TestResultRow, TestStatus};
    use serde_json::json;

    fn row(id: &str, duration_ms: Option<u64>, cached: bool, status: TestStatus) -> TestResultRow {
        TestResultRow {
            test_id: id.to_string(),
            status,
            score: None,
            cached,
            message: String::new(),
            details: json!({}),
            duration_ms,
            fingerprint: None,
            skip_reason: None,
            attempts: None,
            error_policy_applied: None,
        }
    }

    #[test]
    fn performance_metrics_include_pipeline_and_report_timings() {
        let artifacts = assay_core::report::RunArtifacts {
            run_id: 1,
            suite: "demo".to_string(),
            results: vec![row("t1", Some(10), true, TestStatus::Pass)],
            order_seed: None,
            runner_clone_ms: Some(13),
        };
        let timings = PipelineTimings {
            total_ms: 120,
            config_load_ms: Some(5),
            ingest_ms: Some(12),
            runner_build_ms: Some(8),
            run_suite_ms: Some(90),
        };

        let performance = build_performance_metrics(&artifacts, Some(&timings), Some(30));

        assert_eq!(performance.total_duration_ms, 150);
        let phases = performance.phase_timings.expect("phase timings");
        assert_eq!(phases.ingest_ms, Some(12));
        assert_eq!(phases.eval_ms, Some(90));
        assert_eq!(phases.report_ms, Some(30));
        assert_eq!(performance.runner_clone_ms, Some(13));
    }

    #[test]
    fn performance_metrics_compute_cache_hit_rate_and_slowest_top5() {
        let artifacts = assay_core::report::RunArtifacts {
            run_id: 2,
            suite: "demo".to_string(),
            results: vec![
                row("t1", Some(80), true, TestStatus::Pass),
                row("t2", Some(20), false, TestStatus::Fail),
                row("t3", Some(60), true, TestStatus::Pass),
                row("t4", Some(10), false, TestStatus::Warn),
                row("t5", Some(40), false, TestStatus::Pass),
                row("t6", Some(50), false, TestStatus::Pass),
            ],
            order_seed: None,
            runner_clone_ms: None,
        };

        let performance = build_performance_metrics(&artifacts, None, Some(7));

        let hit_rate = performance.cache_hit_rate.expect("cache hit rate");
        assert!((hit_rate - (2.0 / 6.0)).abs() < f64::EPSILON);

        let slowest = performance.slowest_tests.expect("slowest tests");
        assert_eq!(slowest.len(), 5);
        assert_eq!(slowest[0].test_id, "t1");
        assert_eq!(slowest[1].test_id, "t3");
        assert_eq!(slowest[2].test_id, "t6");
        assert_eq!(slowest[3].test_id, "t5");
        assert_eq!(slowest[4].test_id, "t2");
    }

    #[test]
    fn performance_metrics_slowest_tie_breaks_by_test_id() {
        let artifacts = assay_core::report::RunArtifacts {
            run_id: 3,
            suite: "demo".to_string(),
            results: vec![
                row("b", Some(42), false, TestStatus::Pass),
                row("a", Some(42), false, TestStatus::Pass),
                row("c", Some(42), false, TestStatus::Pass),
            ],
            order_seed: None,
            runner_clone_ms: None,
        };

        let performance = build_performance_metrics(&artifacts, None, Some(1));
        let slowest = performance.slowest_tests.expect("slowest tests");
        assert_eq!(slowest.len(), 3);
        assert_eq!(slowest[0].test_id, "a");
        assert_eq!(slowest[1].test_id, "b");
        assert_eq!(slowest[2].test_id, "c");
    }
}
