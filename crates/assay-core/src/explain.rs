//! Trace explanation and visualization
//!
//! Stable facade for the Wave4 Step3 split. Public symbols remain under
//! `crate::explain::*` while implementation lives in `explain_next/*`.

#[path = "explain_next/mod.rs"]
mod explain_next;

pub use explain_next::{
    ExplainedStep, RuleEvaluation, StepVerdict, ToolCall, TraceExplainer, TraceExplanation,
};
