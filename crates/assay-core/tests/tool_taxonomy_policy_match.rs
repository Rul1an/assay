use assay_core::mcp::decision::{Decision, NullDecisionEmitter};
use assay_core::mcp::jsonrpc::JsonRpcRequest;
use assay_core::mcp::policy::{McpPolicy, PolicyDecision, PolicyState};
use assay_core::mcp::tool_call_handler::{HandleResult, ToolCallHandler, ToolCallHandlerConfig};
use assay_core::mcp::{ToolRuleSelector, ToolTaxonomy};
use serde_json::json;
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

fn taxonomy_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/ci/fixtures/tool_taxonomy/policy_tool_class_block_network.yaml")
}

fn tool_call_request(tool: &str) -> JsonRpcRequest {
    JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        method: "tools/call".to_string(),
        params: json!({
            "name": tool,
            "arguments": {}
        }),
        id: Some(json!(1)),
    }
}

#[test]
fn tool_taxonomy_policy_match_alt_sink_matches_shared_class() {
    let mut tool_classes: HashMap<String, BTreeSet<String>> = HashMap::new();
    tool_classes.insert(
        "web_search".to_string(),
        BTreeSet::from(["sink:network".to_string()]),
    );
    tool_classes.insert(
        "web_search_alt".to_string(),
        BTreeSet::from(["sink:network".to_string()]),
    );

    let taxonomy = ToolTaxonomy { tool_classes };
    let selector = ToolRuleSelector::new(None, Some("sink:network".to_string()));

    let alt_ctx = taxonomy.context("web_search_alt");
    let alt_match = selector.matches(&alt_ctx);
    assert!(alt_match.matched);
    assert_eq!(alt_match.matched_classes, vec!["sink:network".to_string()]);

    let primary_ctx = taxonomy.context("web_search");
    assert!(selector.matches(&primary_ctx).matched);
}

#[test]
fn tool_taxonomy_policy_match_class_is_exact_string() {
    let taxonomy = ToolTaxonomy::default();
    let selector = ToolRuleSelector::new(None, Some("sink:network".to_string()));
    let ctx = taxonomy.context("web_search_alt");
    assert!(!selector.matches(&ctx).matched);
}

#[test]
fn tool_taxonomy_policy_match_policy_file_blocks_alt_sink_by_class() {
    let policy = McpPolicy::from_file(&taxonomy_fixture_path()).expect("fixture policy");
    let mut state = PolicyState::default();

    let evaluation = policy.evaluate_with_metadata("web_search_alt", &json!({}), &mut state, None);

    match evaluation.decision {
        PolicyDecision::Deny { code, .. } => assert_eq!(code, "E_TOOL_DENIED"),
        other => panic!("expected class-based deny, got {:?}", other),
    }

    assert_eq!(
        evaluation.metadata.tool_classes,
        vec!["sink:network".to_string()]
    );
    assert_eq!(
        evaluation.metadata.matched_tool_classes,
        vec!["sink:network".to_string()]
    );
    assert_eq!(evaluation.metadata.match_basis.as_str(), Some("class"));
    assert_eq!(
        evaluation.metadata.matched_rule.as_deref(),
        Some("tools.deny_classes")
    );
}

#[test]
fn tool_taxonomy_policy_match_handler_decision_event_records_classes() {
    let policy = McpPolicy::from_file(&taxonomy_fixture_path()).expect("fixture policy");
    let handler = ToolCallHandler::new(
        policy,
        None,
        Arc::new(NullDecisionEmitter),
        ToolCallHandlerConfig {
            event_source: "assay://tool-taxonomy-b1".to_string(),
            ..ToolCallHandlerConfig::default()
        },
    );

    let result = handler.handle_tool_call(
        &tool_call_request("web_search_alt"),
        &mut PolicyState::default(),
        None,
        None,
        None,
    );

    match result {
        HandleResult::Deny { decision_event, .. } => {
            assert_eq!(decision_event.data.decision, Decision::Deny);
            assert_eq!(
                decision_event.data.tool_classes,
                vec!["sink:network".to_string()]
            );
            assert_eq!(
                decision_event.data.matched_tool_classes,
                vec!["sink:network".to_string()]
            );
            assert_eq!(decision_event.data.match_basis.as_deref(), Some("class"));
            assert_eq!(
                decision_event.data.matched_rule.as_deref(),
                Some("tools.deny_classes")
            );
        }
        other => panic!("expected deny result, got {:?}", other),
    }
}
