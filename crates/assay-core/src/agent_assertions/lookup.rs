//! Typed causes for a stored-episode lookup.
//!
//! Missing input and ambiguous input are these variants. A database failure is
//! not: it stays the database error, and callers tell the cases apart by type.

use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum EpisodeLookupError {
    #[error("E_TRACE_EPISODE_MISSING: No episode found for run_id={run_id} test_id={test_id}")]
    Missing { run_id: i64, test_id: String },

    #[error(
        "E_TRACE_EPISODE_MISSING: No episode found for test_id={test_id} (fallback check) : {detail}"
    )]
    FallbackMissing { test_id: String, detail: String },

    #[error(
        "E_TRACE_EPISODE_AMBIGUOUS: Multiple episodes ({count}) found for run_id={run_id} test_id={test_id}"
    )]
    Ambiguous {
        run_id: i64,
        test_id: String,
        count: usize,
    },
}

impl EpisodeLookupError {
    /// True for an absent episode. Ambiguous input and database errors are not.
    pub(crate) fn is_missing(&self) -> bool {
        matches!(self, Self::Missing { .. } | Self::FallbackMissing { .. })
    }
}
