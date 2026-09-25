//! Typed not-evaluated companion for stored-episode assertion rows (#3117).
//!
//! A suite test whose assertions never evaluated — no stored episode matched
//! its id, or more than one did — carries `details["assertions_not_evaluated"]`
//! with a `kind` and a prose `remedy`. The `kind` values live beside the
//! warning vocabulary in [`crate::report::exercised`]; the prose remedies live
//! here, next to the single functions both the row writer and the
//! `ReasonCode::next_step` arms call. One rule, one function: a second
//! spelling of either sentence in either place is free to drift from the
//! other, and the vocabulary inventory scans `exercised.rs` for warning codes,
//! so prose remedies must not live there.

/// Prose remedy for a suite test id that matched no stored episode.
pub const EPISODE_MISSING_REMEDY: &str = "the episode's meta.test_id must match the suite test id";

/// Prose remedy for a suite test id that matched more than one stored episode.
pub const EPISODE_AMBIGUOUS_REMEDY: &str =
    "keep a single stored episode whose meta.test_id is the suite test id";

/// The single source for the missing-episode remedy.
///
/// Called by both the row writer (`engine::runner_next::assertions`) and the
/// `ReasonCode::ETraceEpisodeMissing` next-step arm, so the two cannot drift.
pub fn episode_missing_remedy() -> &'static str {
    EPISODE_MISSING_REMEDY
}

/// The single source for the ambiguous-episode remedy.
///
/// Called by both the row writer (`engine::runner_next::assertions`) and the
/// `ReasonCode::ETraceEpisodeAmbiguous` next-step arm, so the two cannot drift.
pub fn episode_ambiguous_remedy() -> &'static str {
    EPISODE_AMBIGUOUS_REMEDY
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_remedies_are_single_sourced() {
        assert_eq!(
            episode_missing_remedy(),
            "the episode's meta.test_id must match the suite test id"
        );
        assert_eq!(
            episode_ambiguous_remedy(),
            "keep a single stored episode whose meta.test_id is the suite test id"
        );
    }
}
