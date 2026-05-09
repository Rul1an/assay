use super::super::Runner;
use crate::model::{TestCase, TestResultRow, TestStatus};

pub(crate) fn apply_agent_assertions_impl(
    runner: &Runner,
    run_id: i64,
    tc: &TestCase,
    final_row: &mut TestResultRow,
) -> anyhow::Result<()> {
    if let Some(assertions) = &tc.assertions {
        if !assertions.is_empty() {
            match crate::agent_assertions::verify_assertions(
                &runner.store,
                run_id,
                &tc.id,
                assertions,
            ) {
                Ok(diags) => {
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
