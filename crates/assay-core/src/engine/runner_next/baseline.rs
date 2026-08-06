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

        // `continue`, not `?`. `?` returns from the whole function, so one metric without a numeric
        // score silently disabled regression checking for every metric after it in the list.
        let Some(score) = details["metrics"][metric_name]["score"].as_f64() else {
            continue;
        };

        // Did this run actually evaluate the metric? #1949 layer 2: a metric that declined the
        // test's `Expected` variant, or found no antecedent, reports `passed` with score 1.0. That
        // number is stable run to run, so it never trips a score regression — and if a metric that
        // used to be exercised stops being so, its score *rises* to 1.0 and the baseline reads the
        // coverage loss as an improvement.
        let exercised = details["metrics"][metric_name]["exercised"].as_str();
        let is_exercised = exercised.is_none_or(|e| e == "exercised");

        // The coverage regression the score comparison cannot see. Warn rather than fail: this is
        // an observation about what ran, and over-eager vacuity detection earns a suppression and
        // takes real findings with it.
        if coverage_regressed(exercised, baseline.was_exercised(&tc.id, metric_name)) {
            return Some((
                TestStatus::Warn,
                format!(
                    "coverage regression: {} was exercised in the baseline and is {} now",
                    metric_name,
                    exercised.unwrap_or("not exercised")
                ),
            ));
        }

        // A score from a metric that evaluated nothing is not evidence about quality, so it is not
        // compared. Continue rather than return, so later metrics are still checked.
        if !is_exercised {
            continue;
        }

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

/// Did this metric stop being evaluated since the baseline?
///
/// Extracted so the loop above and the tests below apply one rule. The asymmetry is the point:
///
/// - a baseline that predates the `exercised` dimension reports `None`, which is *no evidence*
///   rather than evidence of no coverage. Treating it as a drop would fire on every historical
///   baseline at once.
/// - a current result with no `exercised` field is treated as exercised, for the same reason in
///   the other direction: a run from an older binary must not read as a coverage loss.
///
/// So this fires only when the baseline positively recorded coverage and this run positively
/// reports its absence.
fn coverage_regressed(current: Option<&str>, baseline_exercised: Option<bool>) -> bool {
    let now_exercised = current.is_none_or(|e| e == "exercised");
    !now_exercised && baseline_exercised == Some(true)
}

#[cfg(test)]
mod coverage_regression_tests {
    use super::coverage_regressed;

    /// The case the score comparison cannot see: a metric that used to run and now declines.
    ///
    /// Its score *rises* to 1.0 when that happens, so a score-only check reads the coverage loss as
    /// an improvement.
    #[test]
    fn a_metric_that_stopped_being_evaluated_is_a_regression() {
        assert!(coverage_regressed(Some("not_applicable"), Some(true)));
        assert!(coverage_regressed(Some("not_exercised"), Some(true)));
    }

    #[test]
    fn a_metric_that_still_runs_is_not_a_regression() {
        assert!(!coverage_regressed(Some("exercised"), Some(true)));
    }

    /// It was already not exercised, so nothing was lost.
    #[test]
    fn a_metric_that_never_ran_is_not_a_regression() {
        assert!(!coverage_regressed(Some("not_applicable"), Some(false)));
    }

    /// A baseline predating the dimension is no evidence, not evidence of absence.
    ///
    /// Without this the check would fire on every historical baseline the first time it ran, which
    /// is the over-eager vacuity detection that earns a suppression and takes real findings with it.
    #[test]
    fn a_baseline_that_says_nothing_is_not_a_regression() {
        assert!(!coverage_regressed(Some("not_applicable"), None));
        assert!(!coverage_regressed(Some("not_exercised"), None));
    }

    /// A run from a binary older than the dimension must not read as a coverage loss either.
    #[test]
    fn a_current_result_that_says_nothing_is_not_a_regression() {
        assert!(!coverage_regressed(None, Some(true)));
    }
}
