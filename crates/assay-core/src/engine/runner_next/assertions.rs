use super::super::Runner;
use crate::model::{LlmResponse, TestCase, TestResultRow, TestStatus};

use crate::agent_assertions::EpisodeLookupError;
use crate::report::exercised::{
    ASSERTIONS_NOT_EVALUATED, ASSERTIONS_NOT_EXERCISED, EPISODE_AMBIGUOUS,
    EPISODE_AMBIGUOUS_REMEDY, EPISODE_MISSING, EPISODE_MISSING_REMEDY,
};

pub(crate) fn apply_agent_assertions_impl(
    runner: &Runner,
    run_id: i64,
    tc: &TestCase,
    resp: &LlmResponse,
    final_row: &mut TestResultRow,
) -> anyhow::Result<()> {
    if let Some(assertions) = &tc.assertions {
        if !assertions.is_empty() {
            match crate::agent_assertions::verify_assertions_with_meta(
                &runner.store,
                run_id,
                &tc.id,
                assertions,
                &resp.meta,
            ) {
                Ok(outcome) => {
                    if let Ok(used) = runner.store.take_latest_stored_episode_used() {
                        if !used.is_empty() {
                            eprintln!(
                                "note: assertions used the latest stored episode per test_id (--latest-stored-episode)"
                            );
                            final_row.details["assertion_episode"] = serde_json::json!({
                                "source": "latest_stored_episode",
                                "test_ids": used,
                            });
                        }
                    }
                    // Recorded before the pass/fail branch below, so a test that both failed one
                    // assertion and never exercised another reports both. The failure is the
                    // louder finding; it is not the only one.
                    if !outcome.not_exercised.is_empty() {
                        final_row.details[ASSERTIONS_NOT_EXERCISED] = serde_json::Value::Array(
                            outcome
                                .not_exercised
                                .iter()
                                .map(|c| {
                                    serde_json::json!({
                                        "assertion": c.assertion,
                                        "reason": c.reason,
                                    })
                                })
                                .collect(),
                        );
                    }

                    let diags = outcome.diagnostics;
                    if !diags.is_empty() {
                        final_row.status = TestStatus::Fail;

                        let diag_json: Vec<serde_json::Value> = diags
                            .iter()
                            .map(|d| serde_json::to_value(d).unwrap_or_default())
                            .collect();

                        final_row.details["assertions"] = serde_json::Value::Array(diag_json);

                        let fail_msg = format!("assertions failed ({})", diags.len());
                        if final_row.message == "ok" {
                            final_row.message = fail_msg;
                        } else {
                            final_row.message = format!("{}; {}", final_row.message, fail_msg);
                        }
                    } else {
                        final_row.details["assertions"] = serde_json::json!({ "passed": true });
                    }
                }
                Err(e) => {
                    // A typed episode-lookup miss means the assertions never
                    // evaluated: the row is `Error`, not `Fail` (#3117, 6.7.0).
                    // Every other evaluator error (notably a database failure)
                    // stays `Fail`.
                    let not_evaluated = e.downcast_ref::<EpisodeLookupError>().is_some();
                    final_row.status = if not_evaluated {
                        TestStatus::Error
                    } else {
                        TestStatus::Fail
                    };
                    final_row.message = format!("assertions error: {}", e);
                    final_row.details["assertions"] = serde_json::json!({ "error": e.to_string() });
                    if let Some(lookup) = e.downcast_ref::<EpisodeLookupError>() {
                        let (kind, remedy) = match lookup {
                            EpisodeLookupError::Missing { .. }
                            | EpisodeLookupError::FallbackMissing { .. } => {
                                (EPISODE_MISSING, EPISODE_MISSING_REMEDY)
                            }
                            EpisodeLookupError::Ambiguous { .. } => {
                                (EPISODE_AMBIGUOUS, EPISODE_AMBIGUOUS_REMEDY)
                            }
                        };
                        final_row.details[ASSERTIONS_NOT_EVALUATED] = serde_json::json!({
                            "evaluated": false,
                            "kind": kind,
                            "remedy": remedy,
                        });
                    }
                }
            }
        }
    }
    Ok(())
}
