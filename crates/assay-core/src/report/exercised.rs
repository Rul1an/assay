//! Companion-cover reporting: the metrics a test asked for that evaluated nothing (#1949, layer 2
//! slice 4).
//!
//! # The condition
//!
//! #2068 gave `MetricResult` its third dimension and `single.rs` writes it into
//! `details["metrics"][…]["exercised"]`. Three values, and only one of them is a finding:
//!
//! | value | meaning | reported here |
//! |---|---|---|
//! | `exercised` | the metric evaluated the response | no |
//! | `not_applicable` | the metric declines this test's `Expected` variant | **no** |
//! | `not_exercised` | the metric accepted this test and evaluated nothing | **yes** |
//!
//! The middle row is the whole reason this module is narrow. All thirteen registered metrics run
//! against every test and twelve of them decline the `Expected` variant, so reporting
//! `not_applicable` would emit twelve findings per test and be suppressed within a day. Thirteen,
//! in fact, for a test whose `Expected` is `JudgeCriteria`: no registered metric matches that
//! variant at all, so every metric declines it. Assertion-based verification has the same warning
//! from the other side: Beer et al. on temporal antecedent failure, and every treatment since,
//! records that over-eager vacuity detection earns a suppression and takes the real findings with
//! it.
//!
//! 2026 hardware-verification work on agentic coverage closure ([arXiv:2604.15657]) splits un-hit
//! coverage along the same seam, and names both halves: a *methodology-bound ceiling* (tied-off
//! hardware, infeasible boundaries, dead code) against a *reasoning frontier* (protocol sequencing,
//! warm-up, narrow timing conditions). `not_applicable` is the first shape and `not_exercised` the
//! second. The disposition below — report one and not the other — is this crate's reading, not the
//! paper's: its taxonomy is about what an agent can reach, not about what a tool should print.
//!
//! Structurally bounded, not merely expected to be quiet: every `not_exercised` site in
//! `assay-metrics` sits *after* the `Expected`-variant match, and one test has one `Expected`. So a
//! test contributes at most one finding, and the findings are then folded by metric and reason
//! rather than listed per test.
//!
//! # Why this is not in `codes::`
//!
//! `assay_core::errors::diagnostic::codes` is inventoried by the field its members reach: SARIF
//! `ruleId` under `tool.driver.name = "assay"`. The route to that field is `build_sarif_diagnostics`
//! (`report/sarif.rs`), and it has exactly one non-test caller, `assay validate --format sarif`.
//!
//! The `run` path does build `Diagnostic`s — the trace client, the agent-assertion matchers, and
//! the pipeline's error classifier all do — so "the run path has no diagnostics" would be false and
//! is not the reason. The reason is narrower and is the one the inventory keys on: none of those
//! reaches `build_sarif_diagnostics`, so a code added to `codes::` for this would be recorded on a
//! surface it never appears on.
//!
//! This writes to the `warnings` array of `run.json` / `summary.json` and to the console summary.
//! That is recorded in the inventory as its own surface. If a run-path diagnostic ever acquires a
//! route to `build_sarif_diagnostics`, this constant belongs in `codes::` and the inventory entry
//! moves with it.
//!
//! # Not a fail
//!
//! Nothing here reads or sets `TestStatus`, and the `warnings` array has never contributed to an
//! exit code. A not-exercised metric leaves a green suite green — which is the point: it is a
//! coverage observation, and a coverage observation that fails a build is a coverage observation
//! people delete.
//!
//! [arXiv:2604.15657]: https://arxiv.org/abs/2604.15657

use crate::metrics_api::Exercised;
use crate::model::TestResultRow;
use std::collections::BTreeMap;

/// The identifier carried by every warning this module produces.
///
/// Named in #1949's layer-2 groundwork. It is a `W_` code by the same convention as
/// `codes::W_CFG_VACUOUS_EXPECTED` — an observation that never decides an exit — but it lives here
/// rather than in that registry, for the reason in the module docs.
pub const W_METRIC_NOT_EXERCISED: &str = "W_METRIC_NOT_EXERCISED";

/// How many test ids a single warning names before it stops and counts the rest.
const MAX_NAMED_TESTS: usize = 3;

/// One metric that evaluated nothing, and the tests that asked it to.
///
/// Folded by `(metric, reason)` rather than emitted per test: a coverage hole is a property of the
/// check, and a suite where sixty tests all fail to exercise `sequence_valid` has one hole, not
/// sixty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotExercised {
    pub metric: String,
    pub reason: String,
    /// Sorted, so the same run reports the same order regardless of how the tests were scheduled.
    pub test_ids: Vec<String>,
}

impl NotExercised {
    /// The warning line for the `warnings` array and the console.
    pub fn render(&self) -> String {
        let named = self
            .test_ids
            .iter()
            .take(MAX_NAMED_TESTS)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        let rest = self.test_ids.len().saturating_sub(MAX_NAMED_TESTS);
        let tail = if rest > 0 {
            format!("{named} and {rest} more")
        } else {
            named
        };
        format!(
            "{}: {} was requested by {} test(s) and evaluated nothing ({}) — {}",
            W_METRIC_NOT_EXERCISED,
            self.metric,
            self.test_ids.len(),
            self.reason,
            tail
        )
    }
}

/// The reason a metric recorded for evaluating nothing, or a stand-in.
///
/// `MetricResult::not_exercised` always carries one, so the fallback is for a details object that
/// has been reshaped since — a missing reason must not silently drop the finding, because the
/// finding is that the check did not run and that is true either way.
const UNRECORDED_REASON: &str = "no reason recorded";

/// Collect the not-exercised findings from a finished run.
///
/// Reads `details["metrics"][…]["exercised"]`, the field `single.rs` writes, rather than taking a
/// second path from `MetricResult`. One producer, one consumer, one spelling: the comparison uses
/// [`Exercised::label`], the same function that wrote the value, so the two cannot drift into
/// disagreeing about what `not_exercised` is called.
pub fn collect(results: &[TestResultRow]) -> Vec<NotExercised> {
    let mut folded: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();

    for row in results {
        let Some(metrics) = row.details.get("metrics").and_then(|m| m.as_object()) else {
            continue;
        };
        for (metric_name, metric) in metrics {
            let label = metric.get("exercised").and_then(|e| e.as_str());
            if label != Some(Exercised::NotExercised.label()) {
                continue;
            }
            let reason = metric
                .get("details")
                .and_then(|d| d.get("reason"))
                .and_then(|r| r.as_str())
                .unwrap_or(UNRECORDED_REASON);
            folded
                .entry((metric_name.clone(), reason.to_string()))
                .or_default()
                .push(row.test_id.clone());
        }
    }

    folded
        .into_iter()
        .map(|((metric, reason), mut test_ids)| {
            test_ids.sort();
            NotExercised {
                metric,
                reason,
                test_ids,
            }
        })
        .collect()
}

/// The findings as warning lines, ready for `RunOutcome::warnings`.
pub fn warnings(results: &[TestResultRow]) -> Vec<String> {
    collect(results).iter().map(NotExercised::render).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::TestStatus;

    /// A row shaped the way `single.rs` writes one: `metrics` keyed by name, each with `exercised`
    /// and a nested `details`.
    fn row(test_id: &str, metrics: serde_json::Value) -> TestResultRow {
        TestResultRow {
            test_id: test_id.to_string(),
            status: TestStatus::Pass,
            score: Some(1.0),
            cached: false,
            message: "ok".into(),
            details: serde_json::json!({ "metrics": metrics }),
            duration_ms: Some(1),
            fingerprint: None,
            skip_reason: None,
            attempts: None,
            error_policy_applied: None,
        }
    }

    fn metric(exercised: Exercised, reason: Option<&str>) -> serde_json::Value {
        let details = match reason {
            Some(r) => serde_json::json!({ "reason": r }),
            None => serde_json::json!({}),
        };
        serde_json::json!({
            "score": 1.0,
            "passed": true,
            "unstable": false,
            "exercised": exercised.label(),
            "details": details
        })
    }

    /// The case the slice exists for: a metric the test asked for, which evaluated nothing.
    #[test]
    fn a_requested_metric_that_evaluated_nothing_is_reported() {
        let rows = vec![row(
            "t1",
            serde_json::json!({
                "sequence_valid": metric(Exercised::NotExercised, Some("no tool calls in the trace"))
            }),
        )];
        let found = collect(&rows);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].metric, "sequence_valid");
        assert_eq!(found[0].reason, "no tool calls in the trace");
        assert_eq!(found[0].test_ids, vec!["t1"]);
    }

    /// The load-bearing exclusion. Twelve of thirteen metrics decline every test's `Expected`
    /// variant, so reporting `not_applicable` would put twelve findings on every passing test and
    /// earn the suppression that the vacuity literature warns about.
    #[test]
    fn a_not_applicable_metric_is_not_a_finding() {
        let rows = vec![row(
            "t1",
            serde_json::json!({
                "must_contain": metric(Exercised::NotApplicable, None),
                "regex_match": metric(Exercised::NotApplicable, None),
                "semantic": metric(Exercised::Exercised, None)
            }),
        )];
        assert!(collect(&rows).is_empty());
    }

    /// A hole is a property of the check, so sixty tests that all miss one metric are one finding.
    #[test]
    fn the_same_metric_across_tests_folds_into_one_finding() {
        let m = || {
            serde_json::json!({
                "tool_output_valid": metric(Exercised::NotExercised, Some("no output schemas configured"))
            })
        };
        let rows = vec![row("t2", m()), row("t1", m()), row("t3", m())];
        let found = collect(&rows);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].test_ids, vec!["t1", "t2", "t3"], "sorted");
    }

    /// Two reasons are two holes even under one metric: "no schemas configured" and "the trace had
    /// no tool calls" are different things to go and fix.
    #[test]
    fn one_metric_with_two_reasons_is_two_findings() {
        let rows = vec![
            row(
                "t1",
                serde_json::json!({ "seq": metric(Exercised::NotExercised, Some("no tool calls")) }),
            ),
            row(
                "t2",
                serde_json::json!({ "seq": metric(Exercised::NotExercised, Some("no policy")) }),
            ),
        ];
        assert_eq!(collect(&rows).len(), 2);
    }

    /// A details object with no `reason` still produces the finding. The finding is that the check
    /// did not run; the reason is context, and losing context must not lose the finding.
    #[test]
    fn a_missing_reason_does_not_drop_the_finding() {
        let rows = vec![row(
            "t1",
            serde_json::json!({ "seq": metric(Exercised::NotExercised, None) }),
        )];
        let found = collect(&rows);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].reason, UNRECORDED_REASON);
    }

    /// A row with no `metrics` object — an error or skip row — is skipped rather than panicking.
    #[test]
    fn a_row_without_metrics_is_skipped() {
        let mut r = row("t1", serde_json::json!({}));
        r.details = serde_json::json!({ "prompt": "hello" });
        assert!(collect(&[r]).is_empty());
    }

    /// The rendered line names the code, the metric, the count and the reason.
    #[test]
    fn the_rendered_warning_names_the_code_metric_count_and_reason() {
        let f = NotExercised {
            metric: "sequence_valid".into(),
            reason: "no tool calls in the trace".into(),
            test_ids: vec!["t1".into(), "t2".into()],
        };
        let line = f.render();
        assert!(line.starts_with("W_METRIC_NOT_EXERCISED: "), "{line}");
        assert!(line.contains("sequence_valid"), "{line}");
        assert!(line.contains("2 test(s)"), "{line}");
        assert!(line.contains("no tool calls in the trace"), "{line}");
        assert!(line.contains("t1, t2"), "{line}");
    }

    /// A wide suite names a few tests and counts the rest, so one hole is one line however many
    /// tests hit it.
    #[test]
    fn a_long_test_list_is_bounded_and_counts_the_remainder() {
        let f = NotExercised {
            metric: "seq".into(),
            reason: "no tool calls".into(),
            test_ids: (1..=10).map(|i| format!("t{i:02}")).collect(),
        };
        let line = f.render();
        assert!(line.contains("t01, t02, t03 and 7 more"), "{line}");
        assert_eq!(line.lines().count(), 1, "one hole is one line");
    }

    /// The reader compares against the writer's own vocabulary rather than a second copy of the
    /// string. If `Exercised::label` is ever respelled, this module follows it instead of silently
    /// matching nothing and reporting a clean run.
    #[test]
    fn the_label_compared_against_is_the_one_the_runner_writes() {
        assert_eq!(Exercised::NotExercised.label(), "not_exercised");
        let rows = vec![row(
            "t1",
            serde_json::json!({ "seq": {
                "exercised": Exercised::NotExercised.label(),
                "details": { "reason": "no tool calls" }
            }}),
        )];
        assert_eq!(collect(&rows).len(), 1);
    }
}
