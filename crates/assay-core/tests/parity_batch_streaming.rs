use assay_core::policy_engine::{evaluate_tool_args, VerdictStatus};
use serde_json::json;

#[test]
fn test_parity_batch_vs_streaming_engine() {
    // 1. Define Policy (Common Source)
    let policy_yaml = r#"
    weather_tool:
      type: object
      properties:
        city:
          type: string
        country:
          type: string
      required: ["city"]
    "#;
    let policy: serde_json::Value = serde_yaml::from_str(policy_yaml).unwrap();

    // 2. Define Inputs (Tool Call)
    let tool_name = "weather_tool";
    let valid_args = json!({ "city": "Amsterdam", "country": "NL" });
    let invalid_args = json!({ "country": "NL" }); // Missing city

    // 3. Batch Simulation: Calling the core engine (as Batch would)
    // In reality, Batch loads policy from file -> parses -> calls metric -> calls engine.
    // Here we simulate the engine call.
    let batch_verdict_valid = evaluate_tool_args(&policy, tool_name, &valid_args);
    let batch_verdict_invalid = evaluate_tool_args(&policy, tool_name, &invalid_args);

    // 4. Streaming Simulation: Calling the core engine (as MCP would)
    // MCP loads policy from file -> parses -> calls engine.
    let streaming_verdict_valid = evaluate_tool_args(&policy, tool_name, &valid_args);
    let streaming_verdict_invalid = evaluate_tool_args(&policy, tool_name, &invalid_args);

    // 5. Parity Assertion
    assert_eq!(
        batch_verdict_valid, streaming_verdict_valid,
        "Batch and Streaming verdicts must match for Valid input"
    );
    assert_eq!(
        batch_verdict_valid.status,
        VerdictStatus::Allowed,
        "Valid input should be Allowed"
    );

    assert_eq!(
        batch_verdict_invalid, streaming_verdict_invalid,
        "Batch and Streaming verdicts must match for Invalid input"
    );
    assert_eq!(
        batch_verdict_invalid.status,
        VerdictStatus::Blocked,
        "Invalid input should be Blocked"
    );
    assert_eq!(
        batch_verdict_invalid.reason_code, "E_ARG_SCHEMA",
        "Reason code should match"
    );
}
