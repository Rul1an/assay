use crate::model::{Expected, LlmResponse, TestCase};
use async_trait::async_trait;

/// Whether a metric actually evaluated anything for this test.
///
/// Orthogonal to `passed`, and deliberately not folded into it. Assertion-based verification
/// settled this decades ago: an assertion with zero attempts is a coverage hole with no
/// verification value, so every assert is paired with a *companion cover* confirming it was
/// genuinely exercised rather than vacuously passed. `passed` answers "did the check hold";
/// this answers "was there a check to hold".
///
/// The distinction is load-bearing here because the runner evaluates all thirteen registered
/// metrics against every test, and a metric that does not handle a test's `Expected` variant used
/// to return `pass(1.0)` — indistinguishable from a metric that ran and was satisfied.
///
/// `NotExercised` is a status and never a failure. Over-eager vacuity detection earns a
/// suppression and takes real findings with it (Beer et al. on temporal antecedent failure), so
/// this reports rather than decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exercised {
    /// The metric does not handle this test's `Expected` variant. Nothing was checked.
    NotApplicable,
    /// The metric applies, but the data it needs never appeared in this run, so its antecedent
    /// never fired. A `trace_must_not_call_tool` naming a tool the agent never had is the shape:
    /// syntactically perfect, permanently vacuous for this trace.
    NotExercised,
    /// Genuinely evaluated against the response.
    Exercised,
}

impl Exercised {
    /// The stable string for this value in `details["metrics"][…]["exercised"]`.
    ///
    /// A vocabulary, not a `Debug` rendering: it reaches `run.json`, so it is an interface. It
    /// lives on the enum rather than beside the writer because there is now a reader —
    /// [`crate::report::exercised`] — and a reader with its own copy of `"not_exercised"` would
    /// match nothing the day the spelling moved, reporting a clean run instead of a broken one.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Exercised => "exercised",
            Self::NotApplicable => "not_applicable",
            Self::NotExercised => "not_exercised",
        }
    }
}

#[derive(Debug, Clone)]
pub struct MetricResult {
    pub score: f64,
    pub passed: bool,
    pub unstable: bool,
    /// Additive on purpose. Existing `passed` readers keep working, and callers that want to know
    /// whether the number means anything now have somewhere to look.
    pub exercised: Exercised,
    pub details: serde_json::Value,
}

impl MetricResult {
    pub fn pass(score: f64) -> Self {
        Self {
            score,
            passed: true,
            unstable: false,
            exercised: Exercised::Exercised,
            details: serde_json::json!({}),
        }
    }
    pub fn fail(score: f64, msg: &str) -> Self {
        Self {
            score,
            passed: false,
            unstable: false,
            exercised: Exercised::Exercised,
            details: serde_json::json!({"message": msg}),
        }
    }
    pub fn unstable(score: f64, msg: &str) -> Self {
        Self {
            score,
            passed: false,
            unstable: true,
            exercised: Exercised::Exercised,
            details: serde_json::json!({"message": msg}),
        }
    }

    /// This metric does not handle the test's `Expected` variant.
    ///
    /// `passed` stays true and the score stays 1.0 so that no existing reader starts failing tests
    /// over a metric that was never asked to run. What changes is that the result now says so, and
    /// the runner no longer lets it set the test's score.
    pub fn not_applicable() -> Self {
        Self {
            score: 1.0,
            passed: true,
            unstable: false,
            exercised: Exercised::NotApplicable,
            details: serde_json::json!({"exercised": "not_applicable"}),
        }
    }

    /// This metric applies, but the run produced nothing for it to check.
    ///
    /// `reason` names the missing antecedent, because "not exercised" without it is the same
    /// unactionable silence the dimension exists to remove.
    pub fn not_exercised(reason: &str) -> Self {
        Self {
            score: 1.0,
            passed: true,
            unstable: false,
            exercised: Exercised::NotExercised,
            details: serde_json::json!({"exercised": "not_exercised", "reason": reason}),
        }
    }

    /// Whether this result's `score` and `passed` describe an actual evaluation.
    pub fn is_exercised(&self) -> bool {
        matches!(self.exercised, Exercised::Exercised)
    }
}

#[async_trait]
pub trait Metric: Send + Sync {
    fn name(&self) -> &'static str;
    async fn evaluate(
        &self,
        tc: &TestCase,
        expected: &Expected,
        resp: &LlmResponse,
    ) -> anyhow::Result<MetricResult>;
}
