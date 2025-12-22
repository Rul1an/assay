pub mod matchers;
pub mod model;

use crate::errors::diagnostic::Diagnostic;
use crate::storage::Store;

pub struct EpisodeGraph {
    pub episode_id: String,
    pub steps: Vec<crate::storage::rows::StepRow>,
    pub tool_calls: Vec<crate::storage::rows::ToolCallRow>,
}

pub fn verify_assertions(
    store: &Store,
    run_id: i64,
    test_id: &str,
    assertions: &[model::TraceAssertion],
) -> anyhow::Result<Vec<Diagnostic>> {
    let graph_res = store.get_episode_graph(run_id, test_id);
    match graph_res {
        Ok(graph) => matchers::evaluate(&graph, assertions),
        Err(e) => {
            // Check if error is ambiguous or missing
            // For now, return Err to platform, but ideally convert to Diagnostic
            Err(e)
        }
    }
}
