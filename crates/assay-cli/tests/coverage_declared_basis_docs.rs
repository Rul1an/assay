#[test]
fn coverage_declared_basis_is_stated_at_both_producers_and_the_contract() {
    let policy_producer = include_str!("../src/cli/commands/mcp/coverage_input.rs");
    let caller_producer = include_str!("../src/cli/commands/coverage/generate.rs");
    let contract = include_str!("../../../docs/architecture/ADR-028-Coverage-Report.md");
    let policy_reference = include_str!("../../../docs/reference/config/policies.md");
    let contract_words = contract.split_whitespace().collect::<Vec<_>>().join(" ");

    assert!(policy_producer.contains("Non-enumerable wildcard patterns are excluded"));
    assert!(policy_producer.contains("enumeration floor"));
    assert!(caller_producer.contains("caller-supplied declarations as literal values"));
    assert!(caller_producer.contains("does not filter wildcard-looking values"));

    for required in [
        "run.source: decision_jsonl",
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
}
