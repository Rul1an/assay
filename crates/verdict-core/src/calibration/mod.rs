pub mod model;
pub mod stats;

use crate::model::TestStatus;
use crate::report::RunArtifacts;
use crate::storage::Store;
use model::CalibrationReport;

pub fn from_run(run: &RunArtifacts, target_tail: f64) -> anyhow::Result<CalibrationReport> {
    let mut agg = stats::Aggregator::new(target_tail);
    for r in &run.results {
        if matches!(
            r.status,
            TestStatus::Pass | TestStatus::Warn | TestStatus::Flaky
        ) {
            stats::ingest_row(&mut agg, r);
        }
    }
    Ok(agg.finish("run.json"))
}

pub fn from_db(
    store: &Store,
    suite: &str,
    last: u32,
    target_tail: f64,
) -> anyhow::Result<CalibrationReport> {
    let mut agg = stats::Aggregator::new(target_tail);
    let rows = store.fetch_recent_results(suite, last)?;
    for r in rows {
        if matches!(
            r.status,
            TestStatus::Pass | TestStatus::Warn | TestStatus::Flaky
        ) {
            stats::ingest_row(&mut agg, &r);
        }
    }
    Ok(agg.finish("eval.db"))
}
