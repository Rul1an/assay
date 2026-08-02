use assay_core::metrics_api::Metric;
use assay_core::model::{Expected, LlmResponse, TestCase, TestInput, ToolCallRecord};
use std::fs::OpenOptions;
use std::io::{ErrorKind, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use super::ArgsValidMetric;
use crate::tool_calls::extract_tool_calls_best_effort;

fn make_expected_with_policy(path: &str) -> Expected {
    Expected::ArgsValid {
        policy: Some(path.to_string()),
        schema: None,
    }
}

fn make_test_case() -> TestCase {
    TestCase {
        id: "t1".to_string(),
        input: TestInput {
            prompt: "ignore".to_string(),
            context: None,
        },
        expected: Expected::default(),
        assertions: None,
        on_error: None,
        tags: vec![],
        metadata: None,
    }
}

fn make_response_with_tool(tool_name: &str, args: serde_json::Value) -> LlmResponse {
    let call = ToolCallRecord {
        id: "1".to_string(),
        tool_name: tool_name.to_string(),
        args,
        result: None,
        error: None,
        index: 0,
        ts_ms: 0,
    };
    let mut meta = serde_json::Map::new();
    meta.insert(
        "tool_calls".to_string(),
        serde_json::to_value(vec![call]).unwrap(),
    );
    LlmResponse {
        meta: serde_json::Value::Object(meta),
        ..Default::default()
    }
}

fn write_temp_policy(contents: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let pid = std::process::id();
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    for attempt in 0..32u32 {
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "assay_args_valid_{}_{}_{}_{}.yaml",
            pid, ts, seq, attempt
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                file.write_all(contents.as_bytes()).unwrap();
                return path.to_string_lossy().to_string();
            }
            Err(err) if err.kind() == ErrorKind::AlreadyExists => continue,
            Err(err) => panic!("failed to create temp policy file: {err}"),
        }
    }
    panic!("failed to allocate unique temp policy file name after retries");
}

#[tokio::test]
async fn structured_policy_denies_tool_from_denylist() {
    let policy_path = write_temp_policy(
        r#"version: "2.0"
tools:
  allow: ["*"]
  deny: ["exec"]
schemas:
  read_file:
    type: object
    required: ["path"]
    properties:
      path:
        type: string
"#,
    );

    let metric = ArgsValidMetric;
    let tc = make_test_case();
    let expected = make_expected_with_policy(&policy_path);
    let resp = make_response_with_tool("exec", serde_json::json!({"command":"rm -rf /"}));

    let result = metric.evaluate(&tc, &expected, &resp).await.unwrap();
    assert!(!result.passed);
    assert_eq!(result.score, 0.0);
    assert!(
        result.details.to_string().contains("E_TOOL_DENIED"),
        "details={}",
        result.details
    );

    let _ = std::fs::remove_file(policy_path);
}

#[tokio::test]
async fn bound_structured_policy_preserves_deny_semantics_inline() {
    let metric = ArgsValidMetric;
    let tc = make_test_case();
    let expected = Expected::ArgsValid {
        policy: None,
        schema: Some(serde_json::json!({
            "version": "2.0",
            "tools": {"allow": ["*"], "deny": ["exec"]},
            "schemas": {
                "read_file": {
                    "type": "object",
                    "required": ["path"],
                    "properties": {"path": {"type": "string"}}
                }
            }
        })),
    };
    let resp = make_response_with_tool("exec", serde_json::json!({"command":"rm -rf /"}));

    let result = metric.evaluate(&tc, &expected, &resp).await.unwrap();
    assert!(!result.passed);
    assert!(
        result.details.to_string().contains("E_TOOL_DENIED"),
        "details={}",
        result.details
    );
}

#[tokio::test]
async fn structured_trivial_schema_still_enforces_unconstrained_deny() {
    let metric = ArgsValidMetric;
    let tc = make_test_case();
    let expected = Expected::ArgsValid {
        policy: None,
        schema: Some(serde_json::json!({
            "version": "2.0",
            "enforcement": {"unconstrained_tools": "deny"},
            "schemas": {"Search": true}
        })),
    };
    let resp = make_response_with_tool("exec", serde_json::json!({"command":"ls"}));

    let result = metric.evaluate(&tc, &expected, &resp).await.unwrap();
    assert!(!result.passed);
    assert!(
        result.details.to_string().contains("E_TOOL_UNCONSTRAINED"),
        "details={}",
        result.details
    );
}

#[tokio::test]
async fn metadata_named_tool_remains_a_schema_map_entry() {
    let metric = ArgsValidMetric;
    let tc = make_test_case();
    let expected = Expected::ArgsValid {
        policy: None,
        schema: Some(serde_json::json!({
            "allow": {"type": "object", "required": ["query"]}
        })),
    };
    let resp = make_response_with_tool("allow", serde_json::json!({}));

    let result = metric.evaluate(&tc, &expected, &resp).await.unwrap();
    assert!(!result.passed);
    assert!(
        result.details.to_string().contains("required property"),
        "details={}",
        result.details
    );
}

#[tokio::test]
async fn tool_named_schemas_requires_an_explicit_policy_discriminant() {
    let metric = ArgsValidMetric;
    let tc = make_test_case();
    let resp = make_response_with_tool("schemas", serde_json::json!({"query": 42}));
    let ambiguous = Expected::ArgsValid {
        policy: None,
        schema: Some(serde_json::json!({
            "schemas": {
                "properties": {
                    "query": {"type": "string"}
                }
            }
        })),
    };
    metric
        .evaluate(&tc, &ambiguous, &resp)
        .await
        .expect_err("schemas-only input must not be assigned a silent interpretation");

    let expected = Expected::ArgsValid {
        policy: None,
        schema: Some(serde_json::json!({
            "version": "2.0",
            "schemas": {
                "schemas": {
                    "properties": {
                        "query": {"type": "string"}
                    }
                }
            }
        })),
    };
    let result = metric.evaluate(&tc, &expected, &resp).await.unwrap();
    assert!(
        !result.passed,
        "the schema for tool `schemas` must be enforced"
    );
}

#[tokio::test]
async fn direct_metric_rejects_policies_that_execution_preflight_rejects() {
    let metric = ArgsValidMetric;
    let tc = make_test_case();
    let resp = make_response_with_tool("Search", serde_json::json!({}));

    for schema in [
        serde_json::json!({"version": "2.0", "allow": ["*"]}),
        serde_json::json!({"version": "2.0", "constraints": {}}),
        serde_json::json!({"version": "2.0", "schemas": {"Search": []}}),
    ] {
        let expected = Expected::ArgsValid {
            policy: None,
            schema: Some(schema),
        };
        metric
            .evaluate(&tc, &expected, &resp)
            .await
            .expect_err("direct metric use must preserve execution validation");
    }

    metric
        .evaluate(
            &tc,
            &Expected::ArgsValid {
                policy: None,
                schema: None,
            },
            &resp,
        )
        .await
        .expect_err("an empty args_valid contract must not pass direct metric use");
}

#[tokio::test]
async fn malformed_present_tool_calls_fail_args_validation() {
    let metric = ArgsValidMetric;
    let tc = make_test_case();
    let expected = Expected::ArgsValid {
        policy: None,
        schema: Some(serde_json::json!({
            "Search": {"type": "object"}
        })),
    };
    let resp = LlmResponse {
        meta: serde_json::json!({"tool_calls": {"tool_name": "Search"}}),
        ..Default::default()
    };

    let result = metric.evaluate(&tc, &expected, &resp).await.unwrap();
    assert!(!result.passed, "malformed presence is not an empty trace");
}

#[tokio::test]
async fn legacy_schema_map_keeps_missing_tool_compat() {
    let policy_path = write_temp_policy(
        r#"read_file:
  type: object
  required: ["path"]
  properties:
    path:
      type: string
"#,
    );

    let metric = ArgsValidMetric;
    let tc = make_test_case();
    let expected = make_expected_with_policy(&policy_path);
    let resp = make_response_with_tool("exec", serde_json::json!({"command":"ls"}));

    let result = metric.evaluate(&tc, &expected, &resp).await.unwrap();
    assert!(
        result.passed,
        "legacy schema-only policy should remain compat"
    );

    let _ = std::fs::remove_file(policy_path);
}

#[tokio::test]
async fn structured_policy_applies_to_minimal_legacy_tool_call_shape() {
    let policy_path = write_temp_policy(
        r#"version: "2.0"
tools:
  allow: ["*"]
  deny: ["exec"]
schemas:
  read_file:
    type: object
    required: ["path"]
    properties:
      path:
        type: string
"#,
    );

    let metric = ArgsValidMetric;
    let tc = make_test_case();
    let expected = make_expected_with_policy(&policy_path);
    let resp = LlmResponse {
        meta: serde_json::json!({
            "tool_calls": [
                {
                    "tool_name": "exec",
                    "args": {"command": "rm -rf /"}
                }
            ]
        }),
        ..Default::default()
    };

    let result = metric.evaluate(&tc, &expected, &resp).await.unwrap();
    assert!(
        !result.passed,
        "minimal tool_call shape must still be enforced"
    );
    assert!(
        result.details.to_string().contains("E_TOOL_DENIED"),
        "details={}",
        result.details
    );

    let _ = std::fs::remove_file(policy_path);
}

#[test]
fn extract_tool_calls_best_effort_preserves_valid_legacy_order_and_field_mapping() {
    let resp = LlmResponse {
        meta: serde_json::json!({
            "tool_calls": [
                {
                    "id": "c0",
                    "tool_name": "alpha",
                    "args": {"k":"v"},
                    "result": {"ok": true},
                    "error": "none",
                    "index": 7,
                    "ts_ms": 42
                },
                {
                    "tool": "beta",
                    "args": ["x"],
                    "error": {"code": "E_FAIL"}
                }
            ]
        }),
        ..Default::default()
    };

    let calls = extract_tool_calls_best_effort(&resp).unwrap();
    assert_eq!(calls.len(), 2);

    assert_eq!(calls[0].tool_name, "alpha");
    assert_eq!(calls[0].id, "c0");
    assert_eq!(calls[0].index, 7);
    assert_eq!(calls[0].ts_ms, 42);
    assert_eq!(calls[0].args, serde_json::json!({"k":"v"}));
    assert_eq!(calls[0].result, Some(serde_json::json!({"ok": true})));
    assert_eq!(calls[0].error, Some(serde_json::json!("none")));

    assert_eq!(calls[1].tool_name, "beta");
    assert_eq!(calls[1].id, "legacy-1");
    assert_eq!(calls[1].index, 1);
    assert_eq!(calls[1].ts_ms, 0);
    assert_eq!(calls[1].args, serde_json::json!(["x"]));
    assert_eq!(calls[1].result, None);
    assert_eq!(calls[1].error, Some(serde_json::json!({"code":"E_FAIL"})));
}

#[test]
fn extract_tool_calls_best_effort_distinguishes_missing_from_non_array() {
    let missing = LlmResponse {
        meta: serde_json::json!({}),
        ..Default::default()
    };
    assert!(extract_tool_calls_best_effort(&missing).unwrap().is_empty());

    let non_array = LlmResponse {
        meta: serde_json::json!({"tool_calls": {"tool_name": "x"}}),
        ..Default::default()
    };
    assert!(extract_tool_calls_best_effort(&non_array).is_err());
}
