use super::super::Runner;
use crate::metrics_api::Metric;
use crate::model::{EvalConfig, TestCase, TestStatus};
use std::sync::Arc;

pub(crate) fn check_baseline_regressions_impl(
    runner: &Runner,
    tc: &TestCase,
    cfg: &EvalConfig,
    details: &serde_json::Value,
    metrics: &[Arc<dyn Metric>],
    baseline: &crate::baseline::Baseline,
) -> Option<(TestStatus, String)> {
    let suite_defaults = cfg.settings.thresholding.as_ref();

    for m in metrics {
        let metric_name = m.name();
        let score = details["metrics"][metric_name]["score"].as_f64()?;

        let (mode, max_drop) =
            resolve_threshold_config_impl(runner, tc, metric_name, suite_defaults);

        if mode == "relative" {
            if let Some(base_score) = baseline.get_score(&tc.id, metric_name) {
                let delta = score - base_score;
                if let Some(drop_limit) = max_drop {
                    if delta < -drop_limit {
                        return Some((
                            TestStatus::Fail,
                            format!(
                                "regression: {} dropped {:.3} (limit: {:.3})",
                                metric_name, -delta, drop_limit
                            ),
                        ));
                    }
                }
            } else {
                return Some((
                    TestStatus::Warn,
                    format!("missing baseline for {}/{}", tc.id, metric_name),
                ));
            }
        }
    }
    None
}

pub(crate) fn resolve_threshold_config_impl(
    _runner: &Runner,
    tc: &TestCase,
    metric_name: &str,
    suite_defaults: Option<&crate::model::ThresholdingSettings>,
) -> (String, Option<f64>) {
    let mut mode = "absolute".to_string();
    let mut max_drop = None;

    if let Some(s) = suite_defaults {
        if let Some(m) = &s.mode {
            mode = m.clone();
        }
        max_drop = s.max_drop;
    }

    if let Some(t) = tc.expected.thresholding_for_metric(metric_name) {
        if t.max_drop.is_some() {
            max_drop = t.max_drop;
        }
    }

    (mode, max_drop)
}
