pub mod engine;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct BundleSummary {
    pub run_id: String,
    pub event_count: usize,
    pub run_root: String,
    pub time_range: Option<(String, String)>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffSet {
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

impl DiffSet {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffSummary {
    pub event_count_delta: i64,
    pub duration_delta: Option<String>,
}

/// One retained event, named by the content id verification recomputed for it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct EventRef {
    pub content_hash: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub seq: u64,
}

/// Whole-record comparison of the retained verified events (#3037).
///
/// `run_root_equal` compares the two verified manifests. `run_root` binds the ordered sequence of
/// per-event content ids, so unequal roots are a sound witness that the retained events differ
/// and equal roots that they do not. `added` / `removed` name the content ids present on one side
/// only, in sequence order; both empty with `run_root_equal == false` means the same events in a
/// different order or multiplicity.
#[derive(Debug, Clone, Serialize)]
pub struct EventDiff {
    pub run_root_equal: bool,
    pub added: Vec<EventRef>,
    pub removed: Vec<EventRef>,
}

impl EventDiff {
    pub fn is_empty(&self) -> bool {
        self.run_root_equal && self.added.is_empty() && self.removed.is_empty()
    }
}

/// The subject-projection report. Kept field-stable: it is exhaustively constructible through the
/// public API, so a new field is a major release. The whole-record comparison lives beside it in
/// [`BundleComparison`].
#[derive(Debug, Clone, Serialize)]
pub struct DiffReport {
    pub baseline: BundleSummary,
    pub candidate: BundleSummary,
    pub summary: DiffSummary,
    pub network: DiffSet,
    pub filesystem: DiffSet,
    pub processes: DiffSet,
}

impl DiffReport {
    /// True when every subject projection is unchanged. This says nothing about the retained
    /// events outside those projections; [`BundleComparison::is_empty`] does.
    pub fn is_empty(&self) -> bool {
        self.network.is_empty() && self.filesystem.is_empty() && self.processes.is_empty()
    }
}

/// Both layers of a bundle comparison: the projection report and the retained events by content
/// id. `#[non_exhaustive]` from the start so the next layer is a minor release.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct BundleComparison {
    #[serde(flatten)]
    pub report: DiffReport,
    pub retained_events: EventDiff,
}

impl BundleComparison {
    /// True only when the retained events compare equal by `run_root` and content id and every
    /// subject projection is unchanged. A projection can be equal while the record differs; the
    /// record cannot be equal while a projection differs, because subjects are hash-bound.
    pub fn is_empty(&self) -> bool {
        self.retained_events.is_empty() && self.report.is_empty()
    }
}
