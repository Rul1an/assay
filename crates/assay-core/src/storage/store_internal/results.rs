//! Results read/write boundary for storage store.
//!
//! Intended ownership (Commit B):
//! - results insert/fetch paths and result row mapping

use crate::model::{AttemptRow, TestResultRow, TestStatus};
use rusqlite::{params, Connection};

pub(crate) fn status_to_outcome_impl(s: &TestStatus) -> &'static str {
    match s {
        TestStatus::Pass => "pass",
        TestStatus::Fail => "fail",
        TestStatus::Flaky => "flaky",
        TestStatus::Warn => "warn",
        TestStatus::Error => "error",
        TestStatus::Skipped => "skipped",
        TestStatus::Unstable => "unstable",
        TestStatus::AllowedOnError => "allowed_on_error",
    }
}

pub(crate) fn parse_attempts_impl(attempts_str: Option<String>) -> Option<Vec<AttemptRow>> {
    attempts_str
        .filter(|s| !s.trim().is_empty())
        .and_then(|s| serde_json::from_str(&s).ok())
}

pub(crate) fn message_and_details_from_attempts_impl(
    attempts: Option<&[AttemptRow]>,
) -> (String, serde_json::Value) {
    attempts
        .and_then(|v| v.last())
        .map(|a| (a.message.clone(), a.details.clone()))
        .unwrap_or_else(|| (String::new(), serde_json::json!({})))
}

pub(crate) fn row_to_test_result_impl(row: &rusqlite::Row<'_>) -> rusqlite::Result<TestResultRow> {
    let attempts = parse_attempts_impl(row.get(4)?);
    let (message, details) = message_and_details_from_attempts_impl(attempts.as_deref());

    Ok(TestResultRow {
        test_id: row.get(0)?,
        status: TestStatus::parse(&row.get::<_, String>(1)?),
        message,
        duration_ms: row.get(2)?,
        details,
        score: row.get(3)?,
        cached: false,
        fingerprint: row.get(5)?,
        skip_reason: row.get(6)?,
        attempts,
        error_policy_applied: None,
    })
}

pub(crate) fn insert_run_row_impl(
    conn: &Connection,
    suite: &str,
    started_at: &str,
    status: &str,
    config_json: Option<&str>,
) -> anyhow::Result<i64> {
    conn.execute(
        "INSERT INTO runs(suite, started_at, status, config_json) VALUES (?1, ?2, ?3, ?4)",
        params![suite, started_at, status, config_json],
    )?;
    Ok(conn.last_insert_rowid())
}
