use serde::{Deserialize, Serialize};

/// Final determination from the judge layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum VerdictStatus {
    /// Clearly passed the rubric.
    Pass,
    /// Clearly failed the rubric.
    Fail,
    /// Uncertain result, falls within the borderline band or judge is unstable.
    Abstain,
}

/// Strategy for handling multiple judge evaluations.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RerunStrategy {
    /// Only run once.
    Single,
    /// Sequential Probability Ratio Test inspired:
    /// Run 1 -> Confident? Stop. Else Run 2 (Swapped) -> Agree? Stop. Else Run 3 -> Majority vote.
    #[default]
    SequentialSprt,
    /// Always run 3 times and take majority.
    AlwaysThree,
}

/// Policy for final verdict when judge remains uncertain (Abstain).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TieBreakPolicy {
    /// Fail the test if the judge is uncertain. (Security posture)
    #[default]
    FailClosed,
    /// Flag as unstable/quarantine but don't hard fail.
    Quarantine,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReliabilityConfig {
    /// Minimum score for "borderline" (inclusive). Default 0.4.
    pub borderline_min: f64,
    /// Maximum score for "borderline" (inclusive). Default 0.6.
    pub borderline_max: f64,
    /// Sequential rerun strategy.
    pub rerun_strategy: RerunStrategy,
    /// Max extra judge calls allowed per test. High-cost protection.
    pub max_extra_calls_per_test: u32,
    /// Total budget for extra judge calls across the whole suite run.
    pub max_extra_calls_per_run: u32,
    /// Policy for final Abstain results.
    pub tie_break: TieBreakPolicy,
    /// Use blind labeling (X/Y) in prompts to mitigate bias.
    pub blind_labeling: bool,
    /// Randomize candidate order (with seed) in prompts.
    pub order_randomized: bool,
    /// Hijack defense: wrap candidates in delimiters and add guard instructions.
    pub hijack_defense: bool,
}

impl Default for ReliabilityConfig {
    fn default() -> Self {
        Self {
            borderline_min: 0.4,
            borderline_max: 0.6,
            rerun_strategy: RerunStrategy::SequentialSprt,
            max_extra_calls_per_test: 2,
            max_extra_calls_per_run: 20,
            tie_break: TieBreakPolicy::FailClosed,
            blind_labeling: true,
            order_randomized: true,
            hijack_defense: true,
        }
    }
}

impl ReliabilityConfig {
    /// Maps a raw probability/score [0.0, 1.0] to a verdict status based on borderline band.
    pub fn assess(&self, score: f64) -> VerdictStatus {
        if score >= self.borderline_min && score <= self.borderline_max {
            VerdictStatus::Abstain
        } else if score > self.borderline_max {
            VerdictStatus::Pass
        } else {
            VerdictStatus::Fail
        }
    }

    /// Determines if a re-evaluation is needed based on current results.
    pub fn triggers_rerun(&self, score: f64, iteration: u32) -> bool {
        match self.rerun_strategy {
            RerunStrategy::Single => false,
            RerunStrategy::AlwaysThree => iteration < 3,
            RerunStrategy::SequentialSprt => {
                score >= self.borderline_min && score <= self.borderline_max
            }
        }
    }
}
