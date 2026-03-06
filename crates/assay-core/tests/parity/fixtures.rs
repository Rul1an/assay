use crate::{CheckInput, CheckType, Outcome, PolicyCheck, ToolCall};

/// Generate a comprehensive set of test cases
pub fn all_test_cases() -> Vec<(PolicyCheck, CheckInput, Outcome)> {
    let mut cases = Vec::new();

    // ArgsValid test cases
    cases.extend(args_valid_cases());
    cases.extend(sequence_valid_cases());
    cases.extend(blocklist_cases());
    cases.extend(edge_cases());

    cases
}

fn args_valid_cases() -> Vec<(PolicyCheck, CheckInput, Outcome)> {
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "percent": { "type": "number", "maximum": 30 },
            "reason": { "type": "string" }
        },
        "required": ["percent", "reason"]
    });

    vec![
        // Pass: valid args
        (
            PolicyCheck {
                id: "args_valid_pass".into(),
                check_type: CheckType::ArgsValid,
                params: serde_json::json!({ "schema": schema }),
            },
            CheckInput {
                tool_name: Some("ApplyDiscount".into()),
                args: Some(serde_json::json!({
                    "percent": 15,
                    "reason": "Loyalty discount"
                })),
                trace: None,
            },
            Outcome::Pass,
        ),
        // Fail: exceeds maximum
        (
            PolicyCheck {
                id: "args_valid_exceed_max".into(),
                check_type: CheckType::ArgsValid,
                params: serde_json::json!({ "schema": schema }),
            },
            CheckInput {
                tool_name: Some("ApplyDiscount".into()),
                args: Some(serde_json::json!({
                    "percent": 50,
                    "reason": "Too much"
                })),
                trace: None,
            },
            Outcome::Fail,
        ),
        // Fail: missing required field
        (
            PolicyCheck {
                id: "args_valid_missing_required".into(),
                check_type: CheckType::ArgsValid,
                params: serde_json::json!({ "schema": schema }),
            },
            CheckInput {
                tool_name: Some("ApplyDiscount".into()),
                args: Some(serde_json::json!({
                    "percent": 10
                    // missing "reason"
                })),
                trace: None,
            },
            Outcome::Fail,
        ),
        // Error: no args provided
        (
            PolicyCheck {
                id: "args_valid_no_args".into(),
                check_type: CheckType::ArgsValid,
                params: serde_json::json!({ "schema": schema }),
            },
            CheckInput {
                tool_name: Some("ApplyDiscount".into()),
                args: None,
                trace: None,
            },
            Outcome::Error,
        ),
        // Edge: exactly at maximum (should pass)
        (
            PolicyCheck {
                id: "args_valid_at_max".into(),
                check_type: CheckType::ArgsValid,
                params: serde_json::json!({ "schema": schema }),
            },
            CheckInput {
                tool_name: Some("ApplyDiscount".into()),
                args: Some(serde_json::json!({
                    "percent": 30,
                    "reason": "Maximum allowed"
                })),
                trace: None,
            },
            Outcome::Pass,
        ),
    ]
}

fn sequence_valid_cases() -> Vec<(PolicyCheck, CheckInput, Outcome)> {
    let rules = serde_json::json!({
        "rules": [
            { "type": "require", "tool": "VerifyIdentity" },
            { "type": "before", "first": "VerifyIdentity", "then": "DeleteAccount" }
        ]
    });

    vec![
        // Pass: correct sequence
        (
            PolicyCheck {
                id: "sequence_valid_pass".into(),
                check_type: CheckType::SequenceValid,
                params: rules.clone(),
            },
            CheckInput {
                tool_name: None,
                args: None,
                trace: Some(vec![
                    ToolCall {
                        tool_name: "VerifyIdentity".into(),
                        args: serde_json::json!({}),
                        timestamp_ms: 1000,
                    },
                    ToolCall {
                        tool_name: "ConfirmAction".into(),
                        args: serde_json::json!({}),
                        timestamp_ms: 2000,
                    },
                    ToolCall {
                        tool_name: "DeleteAccount".into(),
                        args: serde_json::json!({}),
                        timestamp_ms: 3000,
                    },
                ]),
            },
            Outcome::Pass,
        ),
        // Fail: missing required tool
        (
            PolicyCheck {
                id: "sequence_missing_required".into(),
                check_type: CheckType::SequenceValid,
                params: rules.clone(),
            },
            CheckInput {
                tool_name: None,
                args: None,
                trace: Some(vec![ToolCall {
                    tool_name: "DeleteAccount".into(),
                    args: serde_json::json!({}),
                    timestamp_ms: 1000,
                }]),
            },
            Outcome::Fail,
        ),
        // Fail: wrong order
        (
            PolicyCheck {
                id: "sequence_wrong_order".into(),
                check_type: CheckType::SequenceValid,
                params: rules.clone(),
            },
            CheckInput {
                tool_name: None,
                args: None,
                trace: Some(vec![
                    ToolCall {
                        tool_name: "DeleteAccount".into(),
                        args: serde_json::json!({}),
                        timestamp_ms: 1000,
                    },
                    ToolCall {
                        tool_name: "VerifyIdentity".into(),
                        args: serde_json::json!({}),
                        timestamp_ms: 2000,
                    },
                ]),
            },
            Outcome::Fail,
        ),
        // Error: no trace
        (
            PolicyCheck {
                id: "sequence_no_trace".into(),
                check_type: CheckType::SequenceValid,
                params: rules.clone(),
            },
            CheckInput {
                tool_name: None,
                args: None,
                trace: None,
            },
            Outcome::Error,
        ),
    ]
}

fn blocklist_cases() -> Vec<(PolicyCheck, CheckInput, Outcome)> {
    let params = serde_json::json!({
        "blocked": ["DeleteDatabase", "DropTable", "ExecuteRawSQL"]
    });

    vec![
        // Pass: allowed tool
        (
            PolicyCheck {
                id: "blocklist_allowed".into(),
                check_type: CheckType::ToolBlocklist,
                params: params.clone(),
            },
            CheckInput {
                tool_name: Some("SelectQuery".into()),
                args: None,
                trace: None,
            },
            Outcome::Pass,
        ),
        // Fail: blocked tool
        (
            PolicyCheck {
                id: "blocklist_blocked".into(),
                check_type: CheckType::ToolBlocklist,
                params: params.clone(),
            },
            CheckInput {
                tool_name: Some("DeleteDatabase".into()),
                args: None,
                trace: None,
            },
            Outcome::Fail,
        ),
        // Fail: another blocked tool
        (
            PolicyCheck {
                id: "blocklist_drop_table".into(),
                check_type: CheckType::ToolBlocklist,
                params: params.clone(),
            },
            CheckInput {
                tool_name: Some("DropTable".into()),
                args: None,
                trace: None,
            },
            Outcome::Fail,
        ),
        // Error: no tool name
        (
            PolicyCheck {
                id: "blocklist_no_tool".into(),
                check_type: CheckType::ToolBlocklist,
                params: params.clone(),
            },
            CheckInput {
                tool_name: None,
                args: None,
                trace: None,
            },
            Outcome::Error,
        ),
    ]
}

fn edge_cases() -> Vec<(PolicyCheck, CheckInput, Outcome)> {
    let _schema = serde_json::json!({
         "type": "string",
         "minLength": 5
    });

    vec![
        // 1. Empty args object where schema expects something
        (
            PolicyCheck {
                id: "edge_empty_args".into(),
                check_type: CheckType::ArgsValid,
                params: serde_json::json!({
                    "schema": { "type": "object", "required": ["foo"] }
                }),
            },
            CheckInput {
                tool_name: Some("EdgeTool".into()),
                args: Some(serde_json::json!({})),
                trace: None,
            },
            Outcome::Fail,
        ),
        // 2. Null args where schema expects object
        (
            PolicyCheck {
                id: "edge_null_args".into(),
                check_type: CheckType::ArgsValid,
                params: serde_json::json!({
                    "schema": { "type": "object" }
                }),
            },
            CheckInput {
                tool_name: Some("EdgeTool".into()),
                args: Some(serde_json::json!(null)),
                trace: None,
            },
            Outcome::Fail,
        ),
        // 3. Deeply nested schema
        (
            PolicyCheck {
                id: "edge_deep_nesting".into(),
                check_type: CheckType::ArgsValid,
                params: serde_json::json!({
                    "schema": {
                        "type": "object",
                        "properties": {
                            "a": { "type": "object", "properties": {
                                "b": { "type": "object", "properties": {
                                    "c": { "type": "integer", "minimum": 0 }
                                }}
                            }}
                        }
                    }
                }),
            },
            CheckInput {
                tool_name: Some("DeepTool".into()),
                args: Some(serde_json::json!({ "a": { "b": { "c": -1 } } })),
                trace: None,
            },
            Outcome::Fail,
        ),
        // 4. Unicode in tool name and args
        (
            PolicyCheck {
                id: "edge_unicode".into(),
                check_type: CheckType::ToolBlocklist,
                params: serde_json::json!({
                    "blocked": ["🔥DangerousTool"]
                }),
            },
            CheckInput {
                tool_name: Some("🔥DangerousTool".into()),
                args: None,
                trace: None,
            },
            Outcome::Fail,
        ),
        // 5. Sequence with repeating tools
        (
            PolicyCheck {
                id: "edge_sequence_repeat".into(),
                check_type: CheckType::SequenceValid,
                params: serde_json::json!({
                    "rules": [{ "type": "require", "tool": "Login" }]
                }),
            },
            CheckInput {
                tool_name: None,
                args: None,
                trace: Some(vec![
                    ToolCall {
                        tool_name: "Login".into(),
                        args: serde_json::json!({}),
                        timestamp_ms: 1,
                    },
                    ToolCall {
                        tool_name: "Login".into(),
                        args: serde_json::json!({}),
                        timestamp_ms: 2,
                    },
                ]),
            },
            Outcome::Pass,
        ),
        // 6. Huge number
        (
            PolicyCheck {
                id: "edge_huge_number".into(),
                check_type: CheckType::ArgsValid,
                params: serde_json::json!({
                     "schema": { "type": "object", "properties": { "val": { "maximum": 100 } } }
                }),
            },
            CheckInput {
                tool_name: Some("MathTool".into()),
                args: Some(serde_json::json!({ "val": 1.0e+25 })),
                trace: None,
            },
            Outcome::Fail,
        ),
    ]
}
