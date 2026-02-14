//! Explain split module (Wave4 Step3).
//!
//! `crate::explain` remains the stable facade.

pub(crate) mod diff;
pub(crate) mod model;
pub(crate) mod render;
pub(crate) mod source;
#[cfg(test)]
pub(crate) mod tests;

pub use model::{ExplainedStep, RuleEvaluation, StepVerdict, ToolCall, TraceExplanation};
pub use source::TraceExplainer;
