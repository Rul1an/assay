//! Eval config keys that load and that nothing acts on.
//!
//! `deny_unknown_fields` and the unknown-key warning only see keys the schema does not have. These
//! keys are in the schema, so a config that sets them loaded clean and ran as if they were absent.
//! [`keys_with_no_effect`] names each one, and the config read refuses the config, in every command
//! that loads one.
//!
//! Each struct below is taken apart field by field, with no `..`. A field added to one of these
//! types does not compile here until it is placed: bound to `_` with the reader that acts on it, or
//! checked and refused. The reader named for a `_` field is the claim a reviewer checks; the
//! compiler only makes sure every field has one.

use crate::judge::reliability::{ReliabilityConfig, TieBreakPolicy};
use crate::model::{EvalConfig, JudgeConfig, Settings, ThresholdingConfig, ThresholdingSettings};
use crate::thresholds::ThresholdConfig;

/// One line per key in `cfg` that nothing acts on: `<path> has no effect: <why>`, in config order.
pub(crate) fn keys_with_no_effect(cfg: &EvalConfig) -> Vec<String> {
    let mut found = Vec::new();
    let mut refuse = |path: String, why: &str| found.push(format!("{path} has no effect: {why}"));

    let EvalConfig {
        version: _, // config::parse_config, the supported-version check
        suite: _,   // the run's name in results, baselines and reports
        model: _,   // the provider client (cli runner_builder)
        settings,
        thresholds,
        otel: _, // the tracing client (cli runner_builder); its own fields are not checked here
        tests,
    } = cfg;

    let ThresholdConfig {
        min_score,
        block_on_warn,
    } = thresholds;
    if min_score.is_some() {
        refuse(
            "thresholds.min_score".into(),
            "nothing reads it. A metric's threshold is its own min_score",
        );
    }
    if block_on_warn.is_some() {
        refuse("thresholds.block_on_warn".into(), "nothing reads it");
    }

    let Settings {
        parallel: _,        // engine runner_next::execute
        timeout_seconds: _, // engine runner_next::execute
        cache: _,           // engine runner_next::single
        seed: _,            // engine runner_next::execute
        judge,
        thresholding,
        on_error: _, // model::validation, the per-test policy fallback
        bail_on_first_failure,
    } = settings;
    if *bail_on_first_failure {
        refuse(
            "settings.bail_on_first_failure".into(),
            "every test runs, whatever fails first",
        );
    }

    // Only `mode: relative` turns the baseline check on (engine runner_next::baseline), and a
    // max_drop is read only by that check.
    let mut relative = false;
    if let Some(ThresholdingSettings {
        mode,
        max_drop,
        min_floor,
    }) = thresholding
    {
        match mode.as_deref() {
            Some("relative") => relative = true,
            Some("absolute") | None => {}
            Some(other) => refuse(
                "settings.thresholding.mode".into(),
                &format!(
                    "{other:?} is neither relative nor absolute, and only relative turns on the \
                     baseline check"
                ),
            ),
        }
        if max_drop.is_some() && !relative {
            refuse(
                "settings.thresholding.max_drop".into(),
                "it applies only with mode: relative",
            );
        }
        if min_floor.is_some() {
            refuse(
                "settings.thresholding.min_floor".into(),
                "the baseline check reads mode and max_drop only",
            );
        }
    }

    if let Some(JudgeConfig {
        rubric_version,
        samples,
        reliability,
    }) = judge
    {
        if rubric_version.is_some() {
            refuse(
                "settings.judge.rubric_version".into(),
                "nothing reads it. A judge metric's rubric_version is set on the test",
            );
        }
        if samples.is_some() {
            refuse(
                "settings.judge.samples".into(),
                "the judge takes its sample count from --judge-samples",
            );
        }
        // Read as a whole into the judge's service config (cli runner_builder).
        let ReliabilityConfig {
            borderline_min: _,           // ReliabilityConfig::assess
            borderline_max: _,           // ReliabilityConfig::assess
            rerun_strategy: _,           // ReliabilityConfig::triggers_rerun
            max_extra_calls_per_test: _, // judge run loop
            max_extra_calls_per_run: _,  // judge run loop
            tie_break,
            blind_labeling: _,   // judge run loop
            order_randomized: _, // judge run loop
            hijack_defense: _,   // judge client
        } = reliability;
        if *tie_break != TieBreakPolicy::FailClosed {
            refuse(
                "settings.judge.reliability.tie_break".into(),
                "an abstaining judge fails the test, so fail_closed is the only value that says \
                 what happens",
            );
        }
    }

    for test in tests {
        let Some(ThresholdingConfig { max_drop }) = test
            .expected
            .own_thresholding()
            .and_then(|(_, thresholding)| thresholding)
        else {
            continue;
        };
        if max_drop.is_some() && !relative {
            refuse(
                format!(
                    "tests.{}.expected.thresholding.max_drop",
                    test.id.escape_debug()
                ),
                "it applies only with settings.thresholding.mode: relative",
            );
        }
    }

    found
}
