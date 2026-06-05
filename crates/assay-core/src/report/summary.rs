//! summary.json output per SPEC-PR-Gate-Outputs-v1.
//!
//! This facade preserves the public `report::summary` module while Wave53 moves
//! implementation behind focused internal modules.

mod metrics;
mod types;
mod writer;

pub use metrics::judge_metrics_from_results;
pub use types::*;
pub use writer::write_summary;
