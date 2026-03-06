use crate::{
    batch, fixtures, streaming, verify_parity, CheckInput, CheckType, Outcome, PolicyCheck,
    ToolCall,
};

#[test]
fn test_all_parity() {
    let cases = fixtures::all_test_cases();
    let mut failures = Vec::new();

    println!("\n========================================");
    println!("Parity Test: Batch vs Streaming");
    println!("========================================\n");

    for (check, input, expected_outcome) in &cases {
        let result = verify_parity(check, input);

        let status = if result.is_identical {
            "✓ PARITY"
        } else {
            failures.push(result.check_id.clone());
            "✗ DIVERGED"
        };

        let outcome_check = if result.batch_result.outcome == *expected_outcome {
            "correct"
        } else {
            "WRONG OUTCOME"
        };

        println!(
            "{} {} [{:?}] ({})",
            status, check.id, result.batch_result.outcome, outcome_check
        );
    }

    println!("\n----------------------------------------");
    println!("Total: {} checks", cases.len());
    println!("Parity: {} passed", cases.len() - failures.len());
    println!("Diverged: {}", failures.len());
    println!("----------------------------------------\n");

    if !failures.is_empty() {
        panic!(
            "PARITY TEST FAILED\n\
             The following checks produced different results in batch vs streaming:\n\
             {:?}\n\n\
             This is a RELEASE BLOCKER.",
            failures
        );
    }

    println!("✓ All parity checks passed!\n");
}

#[test]
fn test_args_valid_parity() {
    let check = PolicyCheck {
        id: "discount_check".into(),
        check_type: CheckType::ArgsValid,
        params: serde_json::json!({
            "schema": {
                "properties": {
                    "percent": { "maximum": 30 }
                },
                "required": ["percent"]
            }
        }),
    };

    let input = CheckInput {
        tool_name: Some("ApplyDiscount".into()),
        args: Some(serde_json::json!({ "percent": 50 })),
        trace: None,
    };

    let result = verify_parity(&check, &input);
    result.assert_parity();
    assert_eq!(result.batch_result.outcome, Outcome::Fail);
}

#[test]
fn test_sequence_parity() {
    let check = PolicyCheck {
        id: "verify_before_delete".into(),
        check_type: CheckType::SequenceValid,
        params: serde_json::json!({
            "rules": [
                { "type": "before", "first": "Verify", "then": "Delete" }
            ]
        }),
    };

    let input = CheckInput {
        tool_name: None,
        args: None,
        trace: Some(vec![
            ToolCall {
                tool_name: "Verify".into(),
                args: serde_json::json!({}),
                timestamp_ms: 1000,
            },
            ToolCall {
                tool_name: "Delete".into(),
                args: serde_json::json!({}),
                timestamp_ms: 2000,
            },
        ]),
    };

    let result = verify_parity(&check, &input);
    result.assert_parity();
    assert_eq!(result.batch_result.outcome, Outcome::Pass);
}

#[test]
fn test_blocklist_parity() {
    let check = PolicyCheck {
        id: "no_delete_db".into(),
        check_type: CheckType::ToolBlocklist,
        params: serde_json::json!({
            "blocked": ["DeleteDatabase"]
        }),
    };

    // Test allowed
    let input_allowed = CheckInput {
        tool_name: Some("SelectQuery".into()),
        args: None,
        trace: None,
    };

    let result = verify_parity(&check, &input_allowed);
    result.assert_parity();
    assert_eq!(result.batch_result.outcome, Outcome::Pass);

    // Test blocked
    let input_blocked = CheckInput {
        tool_name: Some("DeleteDatabase".into()),
        args: None,
        trace: None,
    };

    let result = verify_parity(&check, &input_blocked);
    result.assert_parity();
    assert_eq!(result.batch_result.outcome, Outcome::Fail);
}

#[test]
fn test_hash_determinism() {
    // Verify that result hashes are deterministic
    let check = PolicyCheck {
        id: "hash_test".into(),
        check_type: CheckType::ArgsValid,
        params: serde_json::json!({ "schema": {} }),
    };

    let input = CheckInput {
        tool_name: None,
        args: Some(serde_json::json!({})),
        trace: None,
    };

    let result1 = batch::evaluate(&check, &input);
    let result2 = batch::evaluate(&check, &input);
    let result3 = streaming::evaluate(&check, &input);

    assert_eq!(result1.result_hash, result2.result_hash);
    assert_eq!(result1.result_hash, result3.result_hash);
}
