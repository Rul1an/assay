#[test]
fn coverage_declared_basis_is_stated_at_both_producers_and_the_contract() {
    let policy_producer = include_str!("../src/cli/commands/mcp/coverage_input.rs");
    let caller_producer = include_str!("../src/cli/commands/coverage/generate.rs");
    let contract = include_str!("../../../docs/architecture/ADR-028-Coverage-Report.md");
    let policy_reference = include_str!("../../../docs/reference/config/policies.md");
    let runbook = include_str!("../../../docs/ops/MCP-TOOL-TAXONOMY-AND-COVERAGE-RUNBOOK.md");
    let policy_words = policy_producer
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let contract_words = contract.split_whitespace().collect::<Vec<_>>().join(" ");

    assert!(policy_words.contains("Every value containing `*` is excluded"));
    assert!(policy_words.contains("enumeration floor"));
    assert!(caller_producer.contains("caller-supplied declarations as literal values"));
    assert!(caller_producer.contains("does not filter wildcard-looking values"));

    for required in [
        "run.source: decision_jsonl",
        "excludes every value containing `*`",
        "floor on the policy's reach",
        "run.source: jsonl",
        "retains caller-supplied declarations literally",
    ] {
        assert!(
            contract_words.contains(required),
            "ADR-028 lost: {required}"
        );
    }

    assert!(policy_reference.contains("A wildcard in `deny` over-blocks"));
    assert!(policy_reference.contains("The same wildcard in `allow`\nover-permits"));
    assert!(runbook.contains(
        "`tools.tools_unknown`: set difference between observed tools and this report's `tools_declared`"
    ));
    assert!(runbook.contains("does not prove that a tool escaped policy matching or enforcement"));
}
