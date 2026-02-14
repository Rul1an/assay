use super::super::Runner;
use super::{errors as errors_next, retry as retry_next};
use crate::attempts::classify_attempts;
use crate::model::{AttemptRow, EvalConfig, LlmResponse, TestCase, TestResultRow, TestStatus};
use crate::on_error::ErrorPolicy;
use crate::quarantine::{QuarantineMode, QuarantineService};
use crate::report::progress::{ProgressEvent, ProgressSink};
use crate::report::RunArtifacts;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio::time::{timeout, Duration};

pub(crate) async fn run_suite_impl(
    runner: &Runner,
    cfg: &EvalConfig,
    progress: Option<ProgressSink>,
) -> anyhow::Result<RunArtifacts> {
    let run_id = runner.store.create_run(cfg)?;

    let parallel = cfg.settings.parallel.unwrap_or(4).max(1);
    let sem = Arc::new(Semaphore::new(parallel));
    let mut join_set = JoinSet::new();

    let mut cfg = cfg.clone();
    if cfg.settings.seed.is_none() {
        let s = rand::random();
        cfg.settings.seed = Some(s);
        eprintln!("Info: No seed provided. Using generated seed: {}", s);
    }

    let mut tests = cfg.tests.clone();
    if let Some(seed) = cfg.settings.seed {
        use rand::seq::SliceRandom;
        use rand::SeedableRng;
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        tests.shuffle(&mut rng);
    }

    let total = tests.len();
    let mut clone_overhead_ms: u128 = 0;
    for tc in tests.iter() {
        let permit = sem.clone().acquire_owned().await?;
        let clone_started = Instant::now();
        let this = clone_for_task_impl(runner);
        clone_overhead_ms = clone_overhead_ms.saturating_add(clone_started.elapsed().as_millis());
        let cfg = cfg.clone();
        let tc = tc.clone();
        join_set.spawn(async move {
            let _permit = permit;
            run_test_with_policy_impl(&this, &cfg, &tc, run_id).await
        });
    }

    let mut rows = Vec::new();
    let mut any_fail = false;
    while let Some(res) = join_set.join_next().await {
        let row = match res {
            Ok(Ok(row)) => row,
            Ok(Err(e)) => TestResultRow {
                test_id: "unknown".into(),
                status: TestStatus::Error,
                score: None,
                cached: false,
                message: format!("task error: {}", e),
                details: serde_json::json!({}),
                duration_ms: None,
                fingerprint: None,
                skip_reason: None,
                attempts: None,
                error_policy_applied: None,
            },
            Err(e) => TestResultRow {
                test_id: "unknown".into(),
                status: TestStatus::Error,
                score: None,
                cached: false,
                message: format!("join error: {}", e),
                details: serde_json::json!({}),
                duration_ms: None,
                fingerprint: None,
                skip_reason: None,
                attempts: None,
                error_policy_applied: None,
            },
        };
        any_fail = any_fail || matches!(row.status, TestStatus::Fail | TestStatus::Error);
        rows.push(row);
        if total > 0 {
            if let Some(ref sink) = progress {
                sink(ProgressEvent {
                    done: rows.len(),
                    total,
                });
            }
        }
    }

    rows.sort_by(|a, b| a.test_id.cmp(&b.test_id));

    runner
        .store
        .finalize_run(run_id, if any_fail { "failed" } else { "passed" })?;
    Ok(RunArtifacts {
        run_id,
        suite: cfg.suite.clone(),
        results: rows,
        order_seed: cfg.settings.seed,
        runner_clone_ms: Some(clone_overhead_ms.min(u128::from(u64::MAX)) as u64),
    })
}

pub(crate) async fn run_test_with_policy_impl(
    runner: &Runner,
    cfg: &EvalConfig,
    tc: &TestCase,
    run_id: i64,
) -> anyhow::Result<TestResultRow> {
    let quarantine = QuarantineService::new(runner.store.clone());
    let q_reason = quarantine.is_quarantined(&cfg.suite, &tc.id)?;
    let error_policy = cfg.effective_error_policy(tc);

    let max_attempts = 1 + runner.policy.rerun_failures;
    let mut attempts: Vec<AttemptRow> = Vec::new();
    let mut last_row: Option<TestResultRow> = None;
    let mut last_output: Option<LlmResponse> = None;

    for i in 0..max_attempts {
        let (row, output) = run_attempt_with_policy_impl(runner, cfg, tc, error_policy).await;
        retry_next::record_attempt_impl(&mut attempts, i + 1, &row);
        last_row = Some(row.clone());
        last_output = Some(output.clone());

        if retry_next::should_stop_retries_impl(row.status) {
            break;
        }
    }

    let class = classify_attempts(&attempts);
    let mut final_row = last_row.unwrap_or_else(|| retry_next::no_attempts_row_impl(tc));
    apply_quarantine_overlay_impl(runner, &mut final_row, q_reason.as_deref());
    retry_next::apply_failure_classification_impl(&mut final_row, class, attempts.len());

    let output = last_output.unwrap_or_else(|| empty_output_for_model_impl(runner, cfg));

    final_row.attempts = Some(attempts.clone());
    runner.apply_agent_assertions(run_id, tc, &mut final_row)?;

    runner
        .store
        .insert_result_embedded(run_id, &final_row, &attempts, &output)?;

    Ok(final_row)
}

pub(crate) async fn run_attempt_with_policy_impl(
    runner: &Runner,
    cfg: &EvalConfig,
    tc: &TestCase,
    error_policy: ErrorPolicy,
) -> (TestResultRow, LlmResponse) {
    match runner.run_test_once(cfg, tc).await {
        Ok(res) => res,
        Err(e) => errors_next::error_row_and_output_impl(cfg, tc, e, error_policy),
    }
}

pub(crate) fn apply_quarantine_overlay_impl(
    runner: &Runner,
    final_row: &mut TestResultRow,
    q_reason: Option<&str>,
) {
    if let Some(reason) = q_reason {
        match runner.policy.quarantine_mode {
            QuarantineMode::Off => {}
            QuarantineMode::Warn => {
                final_row.status = TestStatus::Warn;
                final_row.message = format!("quarantined: {}", reason);
            }
            QuarantineMode::Strict => {
                final_row.status = TestStatus::Fail;
                final_row.message = format!("quarantined (strict): {}", reason);
            }
        }
    }
}

pub(crate) fn empty_output_for_model_impl(runner: &Runner, cfg: &EvalConfig) -> LlmResponse {
    LlmResponse {
        text: "".into(),
        provider: runner.client.provider_name().to_string(),
        model: cfg.model.clone(),
        cached: false,
        meta: serde_json::json!({}),
    }
}

pub(crate) async fn call_llm_impl(
    runner: &Runner,
    cfg: &EvalConfig,
    tc: &TestCase,
) -> anyhow::Result<LlmResponse> {
    let t = cfg.settings.timeout_seconds.unwrap_or(30);
    let fut = runner
        .client
        .complete(&tc.input.prompt, tc.input.context.as_deref());
    let resp = timeout(Duration::from_secs(t), fut).await??;
    Ok(resp)
}

pub(crate) fn clone_for_task_impl(runner: &Runner) -> Runner {
    Runner {
        store: runner.store.clone(),
        cache: runner.cache.clone(),
        client: runner.client.clone(),
        metrics: runner.metrics.clone(),
        policy: runner.policy.clone(),
        _network_guard: None,
        embedder: runner.embedder.clone(),
        refresh_embeddings: runner.refresh_embeddings,
        incremental: runner.incremental,
        refresh_cache: runner.refresh_cache,
        judge: runner.judge.clone(),
        baseline: runner.baseline.clone(),
    }
}
