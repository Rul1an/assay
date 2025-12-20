use std::sync::Arc;

use verdict_core::metrics_api::Metric;

mod must_contain;
mod must_not_contain;
mod regex_match;

pub fn default_metrics() -> Vec<Arc<dyn Metric>> {
    vec![
        Arc::new(must_contain::MustContainMetric),
        Arc::new(must_not_contain::MustNotContainMetric),
        regex_match::metric(),
    ]
}
