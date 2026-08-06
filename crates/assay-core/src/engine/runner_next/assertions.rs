use super::super::Runner;
use crate::model::{LlmResponse, TestCase, TestResultRow, TestStatus};

use crate::report::exercised::ASSERTIONS_NOT_EXERCISED;

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
                    final_row.status = TestStatus::Fail;
                    final_row.message = format!("assertions error: {}", e);
                    final_row.details["assertions"] = serde_json::json!({ "error": e.to_string() });
                }
            }
        }
    }
    Ok(())
}
