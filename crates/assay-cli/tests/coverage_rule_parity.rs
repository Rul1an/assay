//! #3166/#3181 parity: one fixture policy + traces through the CLI binary,
//! the MCP `assay_check_coverage` tool (with `rules_triggered` omitted) and
//! the shared `coverage::triggered_rules` function assert identical rule
//! coverage and ids.
//!
//! Fixture: `tests/fixtures/coverage/rule_parity/`. Trace A is
//! `[Search, Create, Notify]`, trace B is `[Search]`. The union triggers
//! seven of the eight rules; only `never_after_read_forbidden_post` (whose
//! trigger `Read` never appears) stays untriggered.

use assert_cmd::Command;
use std::collections::HashSet;

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/coverage/rule_parity")
        .join(name)
}

/// The rule block every leg must produce: 7/8 with the single genuinely
/// untriggered rule named by the analyzer id.
fn assert_rule_block(block: &serde_json::Value) {
    assert_eq!(block["total_rules"].as_u64().unwrap(), 8, "{block}");
    assert_eq!(block["rules_triggered"].as_u64().unwrap(), 7, "{block}");
    assert_eq!(block["coverage_pct"].as_f64().unwrap(), 87.5, "{block}");
    assert_eq!(
        block["untriggered_rules"],
        serde_json::json!(["never_after_read_forbidden_post"]),
        "{block}"
    );
}

fn assert_tool_block(block: &serde_json::Value) {
    assert_eq!(
        block["total_tools_in_policy"].as_u64().unwrap(),
        6,
        "{block}"
    );
    assert_eq!(
        block["tools_seen_in_traces"].as_u64().unwrap(),
        3,
        "{block}"
    );
    assert_eq!(block["coverage_pct"].as_f64().unwrap(), 50.0, "{block}");
}

fn cli_rule_and_tool_blocks() -> (serde_json::Value, serde_json::Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_assay"))
        .arg("coverage")
        .arg("--policy")
        .arg(fixture("policy.yaml"))
        .arg("--traces")
        .arg(fixture("traces.jsonl"))
        .arg("--format")
        .arg("json")
        .output()
        .expect("CLI coverage runs");
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("CLI prints a JSON report");
    (
        report["rule_coverage"].clone(),
        report["tool_coverage"].clone(),
    )
}

#[test]
fn cli_binary_measures_fixture_rule_coverage() {
    let (rule, tool) = cli_rule_and_tool_blocks();
    assert_rule_block(&rule);
    assert_tool_block(&tool);
}

#[tokio::test]
async fn mcp_tool_derives_fixture_rule_coverage_when_omitted() {
    use assay_mcp_server::cache::PolicyCaches;
    use assay_mcp_server::config::ServerConfig;
    use assay_mcp_server::tools::check_coverage::check_coverage;
    use assay_mcp_server::tools::ToolContext;

    let policy_root = fixture("");
    let ctx = ToolContext {
        policy_root_canon: policy_root.canonicalize().unwrap(),
        policy_root: policy_root.clone(),
        cfg: ServerConfig::default(),
        caches: PolicyCaches::new(100),
    };

    // `rules_triggered` omitted on both traces: the server measures it.
    let args = serde_json::json!({
        "policy": "policy.yaml",
        "traces": [
            { "id": "A", "tools": ["Search", "Create", "Notify"] },
            { "id": "B", "tools": ["Search"] },
        ],
        "threshold": 80.0,
    });
    let result = check_coverage(&ctx, &args).await.unwrap();
    assert_rule_block(&result["rule_coverage"]);
    assert_tool_block(&result["tool_coverage"]);

    // A supplied list still wins: the tool stays a pure consumer for callers
    // that hand-compute ids.
    let args = serde_json::json!({
        "policy": "policy.yaml",
        "traces": [
            {
                "id": "A",
                "tools": ["Search", "Create", "Notify"],
                "rules_triggered": ["require_audit"],
            },
        ],
        "threshold": 80.0,
    });
    let result = check_coverage(&ctx, &args).await.unwrap();
    assert_eq!(
        result["rule_coverage"]["rules_triggered"].as_u64().unwrap(),
        1
    );
    assert_eq!(
        result["rule_coverage"]["untriggered_rules"]
            .as_array()
            .unwrap()
            .len(),
        7
    );
}

#[test]
fn shared_function_matches_fixture_rule_coverage() {
    let content = std::fs::read_to_string(fixture("policy.yaml")).unwrap();
    let policy: assay_core::model::Policy = serde_yaml::from_str(&content).unwrap();

    // The six lines the SDK adapter becomes, restated against the same calls:
    // ordered `SequenceCall`s through the shared function, then analyze.
    let trace_calls: &[&[&str]] = &[&["Search", "Create", "Notify"], &["Search"]];
    let mut records = Vec::new();
    for (i, names) in trace_calls.iter().enumerate() {
        let calls: Vec<assay_core::sequence_eval::SequenceCall> = names
            .iter()
            .map(|s| assay_core::sequence_eval::SequenceCall::named(*s))
            .collect();
        let rules_triggered: HashSet<String> =
            assay_core::coverage::triggered_rules(&policy, &calls);
        records.push(assay_core::coverage::TraceRecord {
            trace_id: format!("trace_{i}"),
            tools_called: names.iter().map(|s| s.to_string()).collect(),
            rules_triggered,
        });
    }

    let report =
        assay_core::coverage::CoverageAnalyzer::from_policy(&policy).analyze(&records, 80.0);
    let report = serde_json::to_value(&report).unwrap();
    assert_rule_block(&report["rule_coverage"]);
    assert_tool_block(&report["tool_coverage"]);
}

#[test]
fn all_three_legs_agree_on_rule_ids() {
    let (cli_rule, _) = cli_rule_and_tool_blocks();
    assert_rule_block(&cli_rule);
    // Legs 2 and 3 assert the same block in their own tests; this test pins
    // the CLI leg to the shared expectation so a drift in any one leg fails
    // the suite, not just its own test.
    assert_eq!(
        cli_rule["untriggered_rules"],
        serde_json::json!(["never_after_read_forbidden_post"])
    );
}
