use super::super::{HandleResult, ToolCallHandler, ToolCallHandlerConfig};
use super::fixtures::{make_tool_call_request, CountingEmitter};
use crate::mcp::decision::{reason_codes, NullDecisionEmitter};
use crate::mcp::policy::{McpPolicy, PolicyState};
use crate::runtime::OperationClass;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

#[test]
fn test_commit_tool_without_mandate_denied() {
    let emitter = Arc::new(CountingEmitter(AtomicUsize::new(0)));
    let policy = McpPolicy::default();

    let config = ToolCallHandlerConfig {
        event_source: "assay://test".to_string(),
        require_mandate_for_commit: true,
        commit_tools: vec!["purchase_*".to_string()],
        write_tools: vec![],
        ..Default::default()
    };

    let handler = ToolCallHandler::new(policy, None, emitter.clone(), config);

    let request = make_tool_call_request("purchase_item", serde_json::json!({}));
    let mut state = PolicyState::default();

    let result = handler.handle_tool_call(&request, &mut state, None, None, None);

    assert!(
        matches!(result, HandleResult::Deny { reason_code, .. } if reason_code == reason_codes::P_MANDATE_REQUIRED)
    );
    assert_eq!(emitter.0.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
fn test_is_commit_tool_matching() {
    let config = ToolCallHandlerConfig {
        commit_tools: vec!["purchase_*".to_string(), "delete_account".to_string()],
        ..Default::default()
    };

    let handler = ToolCallHandler::new(
        McpPolicy::default(),
        None,
        Arc::new(NullDecisionEmitter),
        config,
    );

    assert!(handler.is_commit_tool("purchase_item"));
    assert!(handler.is_commit_tool("purchase_subscription"));
    assert!(handler.is_commit_tool("delete_account"));
    assert!(!handler.is_commit_tool("search_products"));
    assert!(!handler.is_commit_tool("purchase"));
}

#[test]
fn test_operation_class_for_tool() {
    let config = ToolCallHandlerConfig {
        commit_tools: vec!["purchase_*".to_string()],
        write_tools: vec!["update_*".to_string(), "create_item".to_string()],
        ..Default::default()
    };
    let handler = ToolCallHandler::new(
        McpPolicy::default(),
        None,
        Arc::new(NullDecisionEmitter),
        config,
    );
    assert_eq!(
        handler.operation_class_for_tool("purchase_item"),
        OperationClass::Commit
    );
    assert_eq!(
        handler.operation_class_for_tool("update_profile"),
        OperationClass::Write
    );
    assert_eq!(
        handler.operation_class_for_tool("create_item"),
        OperationClass::Write
    );
    assert_eq!(
        handler.operation_class_for_tool("read_file"),
        OperationClass::Read
    );
}
