use std::sync::Arc;

use assay_core::metrics_api::Metric;

mod json_schema;
mod judge;
mod must_contain;
mod must_not_contain;
mod regex_match;
mod semantic;

pub mod args_valid;
pub mod sequence_valid;
pub mod tool_blocklist;
pub mod usage;

pub fn default_metrics() -> Vec<Arc<dyn Metric>> {
    vec![
        Arc::new(must_contain::MustContainMetric),
        Arc::new(must_not_contain::MustNotContainMetric),
        regex_match::metric(),
        json_schema::metric(),
        Arc::new(semantic::SemanticSimilarityMetric),
        Arc::new(judge::FaithfulnessMetric),
        Arc::new(judge::RelevanceMetric),
        Arc::new(args_valid::ArgsValidMetric),
        Arc::new(sequence_valid::SequenceValidMetric),
        Arc::new(tool_blocklist::ToolBlocklistMetric),
    ]
}
